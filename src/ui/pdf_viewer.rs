use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{AnnotationMode, PdfViewState};
use rotero_models::{Annotation, AnnotationType};

#[component]
pub fn PdfViewer() -> Element {
    let pdf_state = use_context::<Signal<PdfViewState>>();
    let state = pdf_state.read();

    if state.pdf_path.is_none() {
        return rsx! {
            div {
                style: "flex: 1; display: flex; align-items: center; justify-content: center; color: #999; font-size: 16px;",
                "Open a PDF to get started"
            }
        };
    }

    let page_count = state.page_count;
    let zoom = state.zoom;
    let show_panel = state.show_annotation_panel;

    rsx! {
        div { class: "pdf-viewer-container",
            style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",

            PdfToolbar { page_count, zoom }

            div { style: "flex: 1; display: flex; overflow: hidden;",
                // Scrollable page area
                div { class: "pdf-pages",
                    style: "flex: 1; overflow-y: auto; background: #e8e8e8; padding: 16px; display: flex; flex-direction: column; align-items: center; gap: 12px;",
                    for (idx, page) in state.rendered_pages.iter().enumerate() {
                        PdfPageWithOverlay {
                            key: "{idx}",
                            page_index: page.page_index,
                            base64_png: page.base64_png.clone(),
                            width: page.width,
                            height: page.height,
                        }
                    }

                    if (state.rendered_pages.len() as u32) < page_count {
                        div { style: "padding: 16px; text-align: center; color: #999;",
                            "Scroll to load more pages..."
                        }
                    }
                }

                // Annotation sidebar panel
                if show_panel {
                    AnnotationPanel {}
                }
            }
        }
    }
}

/// A single PDF page with an interactive annotation overlay.
#[component]
fn PdfPageWithOverlay(page_index: u32, base64_png: String, width: u32, height: u32) -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let db = use_context::<Database>();

    let state = pdf_state.read();
    let mode = state.annotation_mode.clone();
    let color = state.annotation_color.clone();
    let paper_id = state.paper_id.unwrap_or(0);
    let page_annotations: Vec<Annotation> = state
        .annotations
        .iter()
        .filter(|a| a.page == page_index as i32)
        .cloned()
        .collect();
    drop(state);

    let cursor = match mode {
        AnnotationMode::Highlight => "crosshair",
        AnnotationMode::Note => "cell",
        AnnotationMode::None => "default",
    };

    rsx! {
        div {
            class: "pdf-page-wrapper",
            style: "position: relative; background: white; box-shadow: 0 2px 8px rgba(0,0,0,0.15); cursor: {cursor};",

            // Base rendered page image
            img {
                src: "data:image/png;base64,{base64_png}",
                width: "{width}",
                height: "{height}",
                style: "display: block; user-select: none;",
                draggable: "false",
            }

            // Annotation overlay: renders existing annotations
            for ann in page_annotations.iter() {
                {render_annotation(ann)}
            }

            // Clickable overlay for creating new annotations
            if mode != AnnotationMode::None {
                div {
                    style: "position: absolute; top: 0; left: 0; width: 100%; height: 100%;",
                    onclick: move |evt| {
                        let coords = evt.element_coordinates();
                        let x = coords.x as f64;
                        let y = coords.y as f64;

                        let ann_type = match mode {
                            AnnotationMode::Highlight => AnnotationType::Highlight,
                            AnnotationMode::Note => AnnotationType::Note,
                            AnnotationMode::None => return,
                        };

                        // Create annotation with click position as geometry
                        let geometry = serde_json::json!({
                            "x": x,
                            "y": y,
                            "width": if ann_type == AnnotationType::Highlight { 200.0 } else { 24.0 },
                            "height": if ann_type == AnnotationType::Highlight { 20.0 } else { 24.0 },
                            "page_width": width,
                            "page_height": height,
                        });

                        let now = chrono::Utc::now();
                        let ann = Annotation {
                            id: None,
                            paper_id,
                            page: page_index as i32,
                            ann_type,
                            color: color.clone(),
                            content: if ann_type == AnnotationType::Note { Some(String::new()) } else { None },
                            geometry,
                            created_at: now,
                            modified_at: now,
                        };

                        let db = db.clone();
                        if let Ok(id) = db.with_conn(|conn| crate::db::annotations::insert_annotation(conn, &ann)) {
                            let mut ann = ann;
                            ann.id = Some(id);
                            pdf_state.with_mut(|s| {
                                s.annotations.push(ann);
                            });
                        }
                    },
                }
            }
        }
    }
}

/// Render a single annotation as an overlay element.
fn render_annotation(ann: &Annotation) -> Element {
    let x = ann.geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = ann.geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w = ann.geometry.get("width").and_then(|v| v.as_f64()).unwrap_or(24.0);
    let h = ann.geometry.get("height").and_then(|v| v.as_f64()).unwrap_or(24.0);
    let color = &ann.color;
    let ann_id = ann.id.unwrap_or(0);

    match ann.ann_type {
        AnnotationType::Highlight => {
            rsx! {
                div {
                    key: "ann-{ann_id}",
                    style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; background: {color}; opacity: 0.35; pointer-events: none; border-radius: 2px;",
                }
            }
        }
        AnnotationType::Note => {
            let has_content = ann.content.as_ref().is_some_and(|c| !c.is_empty());
            let icon_bg = if has_content { color.as_str() } else { "#fbbf24" };
            rsx! {
                div {
                    key: "ann-{ann_id}",
                    style: "position: absolute; left: {x}px; top: {y}px; width: 20px; height: 20px; background: {icon_bg}; border-radius: 4px; border: 1px solid rgba(0,0,0,0.2); cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 12px; pointer-events: auto;",
                    title: "{ann.content.as_deref().unwrap_or(\"Empty note\")}",
                    "N"
                }
            }
        }
        AnnotationType::Area => {
            rsx! {
                div {
                    key: "ann-{ann_id}",
                    style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; border: 2px solid {color}; pointer-events: none;",
                }
            }
        }
    }
}

/// Toolbar with annotation controls.
#[component]
fn PdfToolbar(page_count: u32, zoom: f32) -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let zoom_percent = (zoom * 100.0 / 1.5) as u32;
    let state = pdf_state.read();
    let mode = state.annotation_mode.clone();
    let current_color = state.annotation_color.clone();
    let show_panel = state.show_annotation_panel;
    let ann_count = state.annotations.len();
    drop(state);

    let highlight_bg = if mode == AnnotationMode::Highlight { "#dbeafe" } else { "#fff" };
    let note_bg = if mode == AnnotationMode::Note { "#dbeafe" } else { "#fff" };

    let colors = vec!["#ffff00", "#ff6b6b", "#51cf66", "#339af0", "#cc5de8", "#ff922b"];

    rsx! {
        div { class: "pdf-toolbar",
            style: "display: flex; align-items: center; gap: 8px; padding: 8px 16px; background: #fff; border-bottom: 1px solid #ddd; font-size: 13px; flex-wrap: wrap;",

            span { style: "color: #666;", "{page_count} pages" }

            // Separator
            div { style: "width: 1px; height: 20px; background: #ddd;" }

            // Annotation mode buttons
            button {
                style: "padding: 4px 10px; border: 1px solid #ddd; background: {highlight_bg}; cursor: pointer; border-radius: 4px; font-size: 12px;",
                onclick: move |_| {
                    pdf_state.with_mut(|s| {
                        s.annotation_mode = if s.annotation_mode == AnnotationMode::Highlight {
                            AnnotationMode::None
                        } else {
                            AnnotationMode::Highlight
                        };
                    });
                },
                "Highlight"
            }
            button {
                style: "padding: 4px 10px; border: 1px solid #ddd; background: {note_bg}; cursor: pointer; border-radius: 4px; font-size: 12px;",
                onclick: move |_| {
                    pdf_state.with_mut(|s| {
                        s.annotation_mode = if s.annotation_mode == AnnotationMode::Note {
                            AnnotationMode::None
                        } else {
                            AnnotationMode::Note
                        };
                    });
                },
                "Note"
            }

            // Color picker
            if mode != AnnotationMode::None {
                div { style: "display: flex; gap: 3px; align-items: center;",
                    for c in colors.iter() {
                        {
                            let c = c.to_string();
                            let c2 = c.clone();
                            let is_selected = c == current_color;
                            let border = if is_selected { "2px solid #333" } else { "1px solid #ccc" };
                            rsx! {
                                div {
                                    style: "width: 18px; height: 18px; border-radius: 50%; background: {c}; border: {border}; cursor: pointer;",
                                    onclick: move |_| {
                                        let color = c2.clone();
                                        pdf_state.with_mut(|s| s.annotation_color = color);
                                    },
                                }
                            }
                        }
                    }
                }
            }

            div { style: "flex: 1;" }

            // Toggle annotations panel
            button {
                style: "padding: 4px 10px; border: 1px solid #ddd; background: #fff; cursor: pointer; border-radius: 4px; font-size: 12px;",
                onclick: move |_| {
                    pdf_state.with_mut(|s| s.show_annotation_panel = !s.show_annotation_panel);
                },
                if show_panel { "Hide Notes ({ann_count})" } else { "Notes ({ann_count})" }
            }

            // Separator
            div { style: "width: 1px; height: 20px; background: #ddd;" }

            // Zoom controls
            button {
                style: "padding: 4px 8px; border: 1px solid #ddd; background: #fff; cursor: pointer; border-radius: 4px;",
                onclick: move |_| {
                    pdf_state.with_mut(|s| s.zoom = (s.zoom - 0.3).max(0.5));
                },
                "-"
            }
            span { style: "min-width: 40px; text-align: center;", "{zoom_percent}%" }
            button {
                style: "padding: 4px 8px; border: 1px solid #ddd; background: #fff; cursor: pointer; border-radius: 4px;",
                onclick: move |_| {
                    pdf_state.with_mut(|s| s.zoom = (s.zoom + 0.3).min(5.0));
                },
                "+"
            }
        }
    }
}

/// Side panel showing all annotations for the current PDF.
#[component]
fn AnnotationPanel() -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let db = use_context::<Database>();
    let state = pdf_state.read();
    let annotations = state.annotations.clone();
    drop(state);

    rsx! {
        div { class: "annotation-panel",
            style: "width: 300px; border-left: 1px solid #ddd; background: #fafafa; overflow-y: auto; display: flex; flex-direction: column;",

            div { style: "padding: 12px 16px; border-bottom: 1px solid #eee; font-weight: 600; font-size: 14px;",
                "Annotations ({annotations.len()})"
            }

            if annotations.is_empty() {
                div { style: "padding: 24px 16px; text-align: center; color: #999; font-size: 13px;",
                    "No annotations yet. Use the Highlight or Note tool to add annotations."
                }
            } else {
                div { style: "flex: 1; overflow-y: auto;",
                    for ann in annotations.iter() {
                        {
                            let ann_id = ann.id.unwrap_or(0);
                            let page = ann.page;
                            let color = ann.color.clone();
                            let ann_type = ann.ann_type;
                            let content = ann.content.clone().unwrap_or_default();
                            let mut editing = use_signal(|| false);
                            let mut edit_value = use_signal(|| content.clone());
                            let db_for_delete = db.clone();
                            let db_for_save = db.clone();

                            let type_label = match ann_type {
                                AnnotationType::Highlight => "Highlight",
                                AnnotationType::Note => "Note",
                                AnnotationType::Area => "Area",
                            };

                            rsx! {
                                div {
                                    key: "panel-ann-{ann_id}",
                                    style: "padding: 10px 16px; border-bottom: 1px solid #eee; font-size: 13px;",

                                    // Header
                                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px;",
                                        div { style: "display: flex; align-items: center; gap: 6px;",
                                            div { style: "width: 10px; height: 10px; border-radius: 50%; background: {color};" }
                                            span { style: "font-weight: 500;", "{type_label}" }
                                            span { style: "color: #999; font-size: 11px;", "p.{page + 1}" }
                                        }
                                        button {
                                            style: "padding: 1px 6px; border: 1px solid #ddd; background: #fff; border-radius: 3px; cursor: pointer; font-size: 11px; color: #c00;",
                                            onclick: move |_| {
                                                let db = db_for_delete.clone();
                                                if let Ok(()) = db.with_conn(|conn| crate::db::annotations::delete_annotation(conn, ann_id)) {
                                                    pdf_state.with_mut(|s| {
                                                        s.annotations.retain(|a| a.id != Some(ann_id));
                                                    });
                                                }
                                            },
                                            "x"
                                        }
                                    }

                                    // Content (editable for notes)
                                    if ann_type == AnnotationType::Note {
                                        if editing() {
                                            div { style: "display: flex; flex-direction: column; gap: 4px;",
                                                textarea {
                                                    style: "width: 100%; min-height: 60px; padding: 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 12px; resize: vertical;",
                                                    value: "{edit_value}",
                                                    oninput: move |evt| edit_value.set(evt.value()),
                                                }
                                                div { style: "display: flex; gap: 4px;",
                                                    button {
                                                        style: "padding: 3px 8px; background: #2563eb; color: white; border: none; border-radius: 3px; cursor: pointer; font-size: 11px;",
                                                        onclick: move |_| {
                                                            let new_content = edit_value();
                                                            let db = db_for_save.clone();
                                                            let content_ref = if new_content.is_empty() { None } else { Some(new_content.as_str()) };
                                                            if let Ok(()) = db.with_conn(|conn| crate::db::annotations::update_annotation_content(conn, ann_id, content_ref)) {
                                                                pdf_state.with_mut(|s| {
                                                                    if let Some(a) = s.annotations.iter_mut().find(|a| a.id == Some(ann_id)) {
                                                                        a.content = if new_content.is_empty() { None } else { Some(new_content.clone()) };
                                                                    }
                                                                });
                                                            }
                                                            editing.set(false);
                                                        },
                                                        "Save"
                                                    }
                                                    button {
                                                        style: "padding: 3px 8px; border: 1px solid #ddd; background: #fff; border-radius: 3px; cursor: pointer; font-size: 11px;",
                                                        onclick: move |_| editing.set(false),
                                                        "Cancel"
                                                    }
                                                }
                                            }
                                        } else {
                                            div {
                                                style: "color: #555; cursor: pointer; padding: 4px; border-radius: 4px; min-height: 20px;",
                                                onclick: move |_| {
                                                    edit_value.set(content.clone());
                                                    editing.set(true);
                                                },
                                                if content.is_empty() {
                                                    span { style: "color: #bbb; font-style: italic;", "Click to add note..." }
                                                } else {
                                                    "{content}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

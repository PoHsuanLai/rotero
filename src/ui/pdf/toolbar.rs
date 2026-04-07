use dioxus::prelude::*;

use crate::app::RenderChannel;
use crate::state::app_state::{AnnotationMode, PdfTabManager, TabId, ViewerToolState};
use rotero_db::Database;

#[component]
pub(crate) fn PdfToolbar(page_count: u32, zoom: f32, tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut tools = use_context::<Signal<ViewerToolState>>();
    let render_ch = use_context::<RenderChannel>();
    let _config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let db = use_context::<Database>();
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let zoom_percent = (zoom * 100.0 / 1.5) as u32;

    let t = tools.read();
    let mode = t.annotation_mode;
    let current_color = t.annotation_color.clone();
    let show_panel = t.show_annotation_panel;
    drop(t);

    let can_undo = undo_stack.read().can_undo();
    let can_redo = undo_stack.read().can_redo();
    let ann_count = tabs.read().tab().annotations.len();

    let btn = |m: AnnotationMode| -> &str {
        if mode == m {
            "btn btn--ghost btn--ghost-active"
        } else {
            "btn btn--ghost"
        }
    };
    let highlight_class = btn(AnnotationMode::Highlight);
    let underline_class = btn(AnnotationMode::Underline);
    let note_class = btn(AnnotationMode::Note);
    let ink_class = btn(AnnotationMode::Ink);
    let text_class = btn(AnnotationMode::Text);
    let colors = [
        "#ffff00", "#ff6b6b", "#51cf66", "#339af0", "#cc5de8", "#ff922b",
    ];

    rsx! {
        div { class: "pdf-toolbar",
            span { class: "toolbar-page-count", "{page_count} pages" }
            div { class: "toolbar-separator" }

            div { class: "toolbar-tooltip", "data-tooltip": "Highlight",
                button {
                    class: "{highlight_class}",
                    onclick: move |_| {
                        tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Highlight { AnnotationMode::None } else { AnnotationMode::Highlight });
                    },
                    span { class: "bi bi-highlighter" }
                }
            }
            div { class: "toolbar-tooltip", "data-tooltip": "Underline",
                button {
                    class: "{underline_class}",
                    onclick: move |_| {
                        tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Underline { AnnotationMode::None } else { AnnotationMode::Underline });
                    },
                    span { class: "bi bi-type-underline" }
                }
            }
            div { class: "toolbar-tooltip", "data-tooltip": "Sticky Note",
                button {
                    class: "{note_class}",
                    onclick: move |_| {
                        tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Note { AnnotationMode::None } else { AnnotationMode::Note });
                    },
                    span { class: "bi bi-sticky" }
                }
            }
            div { class: "toolbar-tooltip", "data-tooltip": "Draw",
                button {
                    class: "{ink_class}",
                    onclick: move |_| {
                        tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Ink { AnnotationMode::None } else { AnnotationMode::Ink });
                    },
                    span { class: "bi bi-pencil" }
                }
            }
            div { class: "toolbar-tooltip", "data-tooltip": "Text",
                button {
                    class: "{text_class}",
                    onclick: move |_| {
                        tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Text { AnnotationMode::None } else { AnnotationMode::Text });
                    },
                    span { class: "bi bi-fonts" }
                }
            }

            if mode != AnnotationMode::None {
                div { class: "toolbar-color-row",
                    for c in colors.iter() {
                        {
                            let c = c.to_string();
                            let c2 = c.clone();
                            let is_selected = c == current_color;
                            let swatch_class = if is_selected { "color-swatch color-swatch--selected" } else { "color-swatch" };
                            rsx! {
                                div {
                                    class: "{swatch_class}", style: "background: {c};",
                                    onclick: move |_| {
                                        let color = c2.clone();
                                        tools.with_mut(|t| t.annotation_color = color);
                                    },
                                }
                            }
                        }
                    }
                }
            }

            div { class: "toolbar-separator" }

            {
                let db_undo = db.clone();
                let db_redo = db.clone();
                let undo_class = if can_undo { "btn btn--ghost btn--sm toolbar-zoom-btn" } else { "btn btn--ghost btn--sm toolbar-zoom-btn btn--disabled" };
                let redo_class = if can_redo { "btn btn--ghost btn--sm toolbar-zoom-btn" } else { "btn btn--ghost btn--sm toolbar-zoom-btn btn--disabled" };
                rsx! {
                    div { class: "toolbar-tooltip", "data-tooltip": "Undo",
                        button {
                            class: "{undo_class}",
                            onclick: move |_| {
                                if !undo_stack.read().can_undo() { return; }
                                let action = undo_stack.with_mut(|s| s.pop_undo());
                                if let Some(action) = action {
                                    let db = db_undo.clone();
                                    spawn(async move {
                                        crate::state::undo::reverse_action(db, &mut tabs, &mut undo_stack, action).await;
                                    });
                                }
                            },
                            span { class: "bi bi-arrow-counterclockwise" }
                        }
                    }
                    div { class: "toolbar-tooltip", "data-tooltip": "Redo",
                        button {
                            class: "{redo_class}",
                        onclick: move |_| {
                            if !undo_stack.read().can_redo() { return; }
                            let action = undo_stack.with_mut(|s| s.pop_redo());
                            if let Some(action) = action {
                                let db = db_redo.clone();
                                spawn(async move {
                                    crate::state::undo::forward_action(db, &mut tabs, &mut undo_stack, action).await;
                                });
                            }
                        },
                        span { class: "bi bi-arrow-clockwise" }
                    }
                    }
                }
            }

            div { class: "toolbar-spacer" }

            button {
                class: "btn btn--ghost",
                onclick: move |_| {
                    let render_tx = render_ch.sender();
                    tabs.with_mut(|m| m.tab_mut().nav.show_thumbnails = !m.tab().nav.show_thumbnails);
                    if tabs.read().tab().render.thumbnails.is_empty() {
                        spawn(async move {
                            let _ = crate::state::commands::load_thumbnails(&render_tx, &mut tabs, tab_id, 0, 50).await;
                        });
                    }
                },
                "Pages"
            }
            button {
                class: "btn btn--ghost",
                onclick: move |_| {
                    let render_tx = render_ch.sender();
                    tabs.with_mut(|m| m.tab_mut().nav.show_outline = !m.tab().nav.show_outline);
                    if tabs.read().tab().nav.outline.is_empty() {
                        spawn(async move {
                            let _ = crate::state::commands::load_outline(&render_tx, &mut tabs, tab_id).await;
                        });
                    }
                },
                "TOC"
            }
            button {
                class: "btn btn--ghost",
                onclick: move |_| {
                    tools.with_mut(|t| t.show_annotation_panel = !t.show_annotation_panel);
                },
                if show_panel { "Hide Notes ({ann_count})" } else { "Notes ({ann_count})" }
            }

            if ann_count > 0 {
                button {
                    class: "btn btn--ghost",
                    onclick: move |_| {
                        let tab = tabs.read().tab().clone();
                        let pdf_path = tab.pdf_path.clone();
                        let annotations = tab.annotations.clone();

                        let default_name = std::path::Path::new(&pdf_path)
                            .file_stem()
                            .map(|s| format!("{}-annotated.pdf", s.to_string_lossy()))
                            .unwrap_or_else(|| "annotated.pdf".to_string());

                        let file = super::super::save_file(&["pdf"], "Export PDF with Annotations", &default_name);

                        if let Some(output_path) = file {
                            let render_tx = render_ch.sender();
                            spawn(async move {
                                let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                                if render_tx.send(crate::state::commands::RenderRequest::GetPageDimensions {
                                    pdf_path: pdf_path.clone(),
                                    reply: reply_tx,
                                }).is_err() {
                                    tracing::error!("Failed to send GetPageDimensions request");
                                    return;
                                }
                                let dims = match tokio::task::spawn_blocking(move || reply_rx.recv()).await {
                                    Ok(Ok(Ok(d))) => d,
                                    _ => {
                                        tracing::error!("Failed to get page dimensions");
                                        return;
                                    }
                                };
                                match rotero_pdf::write_annotations(
                                    std::path::Path::new(&pdf_path),
                                    &output_path,
                                    &annotations,
                                    &dims,
                                ) {
                                    Ok(()) => tracing::info!("Exported annotated PDF to {:?}", output_path),
                                    Err(e) => tracing::error!("Failed to export annotated PDF: {e}"),
                                }
                            });
                        }
                    },
                    "Export PDF"
                }
            }

            div { class: "toolbar-separator" }

            button {
                class: "btn btn--ghost btn--sm toolbar-zoom-btn",
                onclick: move |_| {
                    let new_zoom = (zoom - 0.3_f32).max(0.5);
                    crate::state::commands::set_zoom(&mut tabs, tab_id, new_zoom);
                },
                span { class: "bi bi-zoom-out" }
            }
            span { class: "toolbar-zoom-value", "{zoom_percent}%" }
            button {
                class: "btn btn--ghost btn--sm toolbar-zoom-btn",
                onclick: move |_| {
                    let new_zoom = (zoom + 0.3_f32).min(5.0);
                    crate::state::commands::set_zoom(&mut tabs, tab_id, new_zoom);
                },
                span { class: "bi bi-zoom-in" }
            }
        }
    }
}

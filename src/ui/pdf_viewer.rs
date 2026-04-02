use dioxus::prelude::*;

use crate::app::RenderChannel;
use crate::db::Database;
use crate::state::app_state::{AnnotationMode, LibraryState, LibraryView, PdfTabManager, ViewerToolState, TabId};
use rotero_models::{Annotation, AnnotationType};

// ── Tab bar ───────────────────────────────────────────────────────

#[component]
pub fn PdfTabBar() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let render_ch = use_context::<RenderChannel>();

    let mgr = tabs.read();
    let tab_info: Vec<(TabId, String, bool)> = mgr.tabs.iter().map(|t| {
        (t.id, t.title.clone(), mgr.active_tab_id == Some(t.id))
    }).collect();
    drop(mgr);

    rsx! {
        div { class: "pdf-tab-bar",
            for (tab_id, title, is_active) in tab_info.iter() {
                {
                    let tab_id = *tab_id;
                    let title = title.clone();
                    let is_active = *is_active;
                    let tab_class = if is_active { "pdf-tab pdf-tab--active" } else { "pdf-tab" };
                    let display_title = if title.len() > 30 {
                        format!("{}...", &title[..27])
                    } else {
                        title
                    };

                    rsx! {
                        div {
                            key: "tab-{tab_id}",
                            class: "{tab_class}",
                            onclick: move |_| {
                                if is_active { return; }
                                // Save scroll position before switching
                                spawn(async move {
                                    let mut eval = document::eval(
                                        "let el = document.getElementById('pdf-pages-container'); el ? el.scrollTop : 0"
                                    );
                                    if let Ok(scroll) = eval.recv::<f64>().await {
                                        tabs.with_mut(|m| {
                                            if let Some(t) = m.active_tab_mut() {
                                                t.view.scroll_top = scroll;
                                            }
                                        });
                                    }
                                    tabs.with_mut(|m| m.switch_to(tab_id));
                                    // Re-render if suspended
                                    let needs = tabs.read().active_tab().map(|t| t.needs_render()).unwrap_or(false);
                                    if needs {
                                        let render_tx = render_ch.sender();
                                        tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                        let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, tab_id).await;
                                    }
                                    // Restore scroll
                                    let scroll_top = tabs.read().active_tab().map(|t| t.view.scroll_top).unwrap_or(0.0);
                                    let js = format!(
                                        "setTimeout(() => {{ let el = document.getElementById('pdf-pages-container'); if (el) el.scrollTop = {}; }}, 50)",
                                        scroll_top
                                    );
                                    let _ = document::eval(&js);
                                });
                            },
                            span { class: "pdf-tab-title", "{display_title}" }
                            button {
                                class: "pdf-tab-close",
                                onclick: move |evt| {
                                    evt.stop_propagation();
                                    tabs.with_mut(|m| m.close_tab(tab_id));
                                    if tabs.read().tabs.is_empty() {
                                        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                                    } else {
                                        // Re-render newly active tab if needed
                                        let needs = tabs.read().active_tab().map(|t| t.needs_render()).unwrap_or(false);
                                        if needs {
                                            let new_id = tabs.read().active_tab_id.unwrap();
                                            let render_tx = render_ch.sender();
                                            tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                            spawn(async move {
                                                let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, new_id).await;
                                            });
                                        }
                                    }
                                },
                                "\u{00d7}"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── Main viewer ───────────────────────────────────────────────────

#[component]
pub fn PdfViewer() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let tools = use_context::<Signal<ViewerToolState>>();
    let render_ch = use_context::<RenderChannel>();
    let mut is_loading = use_signal(|| false);

    let mgr = tabs.read();
    let Some(tab) = mgr.active_tab() else {
        return rsx! {
            div { class: "pdf-viewer-empty", "Open a PDF to get started" }
        };
    };

    let tab_id = tab.id;
    let is_initial_loading = tab.is_loading && tab.render.rendered_pages.is_empty();
    let page_count = tab.page_count;
    let zoom = tab.view.zoom;
    let render_zoom = tab.view.render_zoom;
    let rendered_count = tab.render.rendered_pages.len() as u32;
    let has_more = rendered_count < page_count;
    let batch_size = tab.view.page_batch_size;
    let show_thumbnails = tab.nav.show_thumbnails;
    let show_outline = tab.nav.show_outline && !tab.nav.outline.is_empty();
    let show_search = tab.search.visible;

    let t = tools.read();
    let show_panel = t.show_annotation_panel;
    drop(t);
    drop(mgr);

    rsx! {
        div {
            class: "pdf-viewer-container",
            tabindex: "0",
            onmounted: move |evt| {
                let _ = evt.data().set_focus(true);
            },
            onkeydown: move |evt| {
                let key = evt.key();
                match key {
                    Key::Character(ref c) if c == "+" || c == "=" => {
                        let new_zoom = (zoom + 0.3_f32).min(5.0);
                        let render_tx = render_ch.sender();
                        spawn(async move {
                            let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom).await;
                        });
                    }
                    Key::Character(ref c) if c == "-" => {
                        let new_zoom = (zoom - 0.3_f32).max(0.5);
                        let render_tx = render_ch.sender();
                        spawn(async move {
                            let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom).await;
                        });
                    }
                    Key::PageDown => {
                        spawn(async move {
                            let _ = document::eval("let el = document.getElementById('pdf-pages-container'); el.scrollBy({ top: el.clientHeight * 0.9, behavior: 'smooth' });");
                        });
                    }
                    Key::PageUp => {
                        spawn(async move {
                            let _ = document::eval("let el = document.getElementById('pdf-pages-container'); el.scrollBy({ top: -el.clientHeight * 0.9, behavior: 'smooth' });");
                        });
                    }
                    Key::Home => {
                        spawn(async move {
                            let _ = document::eval("let el = document.getElementById('pdf-pages-container'); el.scrollTo({ top: 0, behavior: 'smooth' });");
                        });
                    }
                    Key::End => {
                        spawn(async move {
                            let _ = document::eval("let el = document.getElementById('pdf-pages-container'); el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });");
                        });
                    }
                    Key::Character(ref c) if c == " " => {
                        if evt.modifiers().shift() {
                            spawn(async move {
                                let _ = document::eval("let el = document.getElementById('pdf-pages-container'); el.scrollBy({ top: -el.clientHeight * 0.9, behavior: 'smooth' });");
                            });
                        } else {
                            spawn(async move {
                                let _ = document::eval("let el = document.getElementById('pdf-pages-container'); el.scrollBy({ top: el.clientHeight * 0.9, behavior: 'smooth' });");
                            });
                        }
                    }
                    Key::Character(ref c) if (c == "f") && (evt.modifiers().meta() || evt.modifiers().ctrl()) => {
                        evt.prevent_default();
                        tabs.with_mut(|m| {
                            let t = m.tab_mut();
                            t.search.visible = !t.search.visible;
                        });
                    }
                    Key::Escape => {
                        tabs.with_mut(|m| {
                            let t = m.tab_mut();
                            if t.search.visible {
                                t.search.visible = false;
                                t.search.query.clear();
                                t.search.matches.clear();
                                t.search.current_index = 0;
                            }
                        });
                    }
                    _ => {}
                }
            },

            PdfToolbar { page_count, zoom, tab_id }

            if show_search {
                PdfSearchBar { tab_id }
            }

            div { class: "pdf-content-area",
                if show_thumbnails {
                    ThumbnailSidebar {}
                }
                if show_outline {
                    OutlinePanel {}
                }
                if is_initial_loading {
                    div { class: "pdf-loading-overlay",
                        div { class: "pdf-loading-spinner" }
                        div { class: "pdf-loading-text", "Loading PDF..." }
                    }
                }

                // Scrollable page area
                div {
                    class: "pdf-pages",
                    id: "pdf-pages-container",
                    onscroll: move |_| {
                        if is_loading() || !has_more {
                            return;
                        }
                        let render_tx = render_ch.sender();
                        let start = rendered_count;
                        let count = batch_size;
                        spawn(async move {
                            let mut result = document::eval(
                                "let el = document.getElementById('pdf-pages-container'); [el.scrollTop, el.clientHeight, el.scrollHeight]"
                            );
                            if let Ok(val) = result.recv::<serde_json::Value>().await {
                                if let Some(arr) = val.as_array() {
                                    let scroll_top = arr[0].as_f64().unwrap_or(0.0);
                                    let client_height = arr[1].as_f64().unwrap_or(0.0);
                                    let scroll_height = arr[2].as_f64().unwrap_or(0.0);

                                    // Save scroll position
                                    tabs.with_mut(|m| {
                                        if let Some(t) = m.active_tab_mut() {
                                            t.view.scroll_top = scroll_top;
                                        }
                                    });

                                    if scroll_top + client_height >= scroll_height - 600.0 {
                                        is_loading.set(true);
                                        let _ = crate::state::commands::render_more_pages(
                                            &render_tx, &mut tabs, tab_id, start, count,
                                        ).await;
                                        is_loading.set(false);
                                    }
                                }
                            }
                        });
                    },

                    {
                        let mgr = tabs.read();
                        let tab = mgr.tab();
                        let pages = tab.render.rendered_pages.clone();
                        drop(mgr);
                        rsx! {
                            for (idx, page) in pages.iter().enumerate() {
                                PdfPageWithOverlay {
                                    key: "{idx}",
                                    page_index: page.page_index,
                                    base64_data: page.base64_data.clone(),
                                    mime: page.mime,
                                    width: page.width,
                                    height: page.height,
                                    zoom,
                                    render_zoom,
                                    tab_id,
                                }
                            }
                        }
                    }

                    if has_more {
                        div { class: "pdf-load-more",
                            if is_loading() { "Loading pages..." } else { "Scroll to load more pages..." }
                        }
                    }
                }

                if show_panel {
                    AnnotationPanel { tab_id }
                }
            }
        }
    }
}

// ── Page overlay ──────────────────────────────────────────────────

#[component]
fn PdfPageWithOverlay(
    page_index: u32,
    base64_data: String,
    mime: &'static str,
    width: u32,
    height: u32,
    zoom: f32,
    render_zoom: f32,
    tab_id: TabId,
) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let tools = use_context::<Signal<ViewerToolState>>();
    let db = use_context::<Database>();

    let mgr = tabs.read();
    let tab = mgr.tab();
    let paper_id = tab.paper_id.unwrap_or(0);
    let page_annotations: Vec<Annotation> = tab.annotations
        .iter().filter(|a| a.page == page_index as i32).cloned().collect();
    let text_segments: Vec<rotero_pdf::TextSegment> = tab.render.text_data
        .get(&page_index).map(|td| td.segments.clone()).unwrap_or_default();
    let search_bounds: Vec<(f64, f64, f64, f64)> = tab.search.matches
        .iter().filter(|m| m.page_index == page_index)
        .flat_map(|m| m.bounds.iter().copied()).collect();
    drop(mgr);

    let t = tools.read();
    let mode = t.annotation_mode;
    let color = t.annotation_color.clone();
    drop(t);

    let cursor = match mode {
        AnnotationMode::Highlight => "crosshair",
        AnnotationMode::Note => "cell",
        AnnotationMode::None => "default",
    };

    let scale_factor = if render_zoom > 0.0 { zoom / render_zoom } else { 1.0 };
    let needs_scaling = (scale_factor - 1.0).abs() > 0.01;
    let wrapper_style = if needs_scaling {
        format!("cursor: {cursor}; transform: scale({scale_factor}); transform-origin: top center;")
    } else {
        format!("cursor: {cursor};")
    };

    rsx! {
        div {
            class: "pdf-page-wrapper",
            style: "{wrapper_style}",

            img {
                class: "pdf-page-img",
                src: "data:{mime};base64,{base64_data}",
                width: "{width}",
                height: "{height}",
                draggable: "false",
            }

            div {
                class: "text-layer",
                style: "width: {width}px; height: {height}px;",
                for (seg_idx, seg) in text_segments.iter().enumerate() {
                    span {
                        key: "text-{page_index}-{seg_idx}",
                        style: "left: {seg.x}px; top: {seg.y}px; width: {seg.width}px; height: {seg.height}px; font-size: {seg.font_size}px;",
                        "{seg.text}"
                    }
                }
            }

            for (si, (sx, sy, sw, sh)) in search_bounds.iter().enumerate() {
                div {
                    key: "search-{page_index}-{si}",
                    style: "position: absolute; left: {sx}px; top: {sy}px; width: {sw}px; height: {sh}px; background: rgba(255, 165, 0, 0.35); pointer-events: none; z-index: 2; border-radius: 2px;",
                }
            }

            for ann in page_annotations.iter() {
                {render_annotation(ann)}
            }

            if mode != AnnotationMode::None {
                {
                    let mut drag_start = use_signal(|| None::<(f64, f64)>);
                    let mut drag_current = use_signal(|| None::<(f64, f64)>);
                    let drag_rect = if mode == AnnotationMode::Highlight {
                        if let (Some(start), Some(current)) = (drag_start(), drag_current()) {
                            let x = start.0.min(current.0);
                            let y = start.1.min(current.1);
                            let w = (start.0 - current.0).abs();
                            let h = (start.1 - current.1).abs();
                            if w > 2.0 || h > 2.0 { Some((x, y, w, h)) } else { None }
                        } else { None }
                    } else { None };

                    rsx! {
                        if let Some((rx, ry, rw, rh)) = drag_rect {
                            div {
                                style: "position: absolute; left: {rx}px; top: {ry}px; width: {rw}px; height: {rh}px; background: {color}; opacity: 0.3; pointer-events: none; z-index: 5; border-radius: 2px;",
                            }
                        }
                        div {
                            class: "annotation-click-overlay",
                            onmousedown: move |evt| {
                                if mode == AnnotationMode::Highlight {
                                    let coords = evt.element_coordinates();
                                    drag_start.set(Some((coords.x, coords.y)));
                                    drag_current.set(Some((coords.x, coords.y)));
                                }
                            },
                            onmousemove: move |evt| {
                                if mode == AnnotationMode::Highlight && drag_start().is_some() {
                                    let coords = evt.element_coordinates();
                                    drag_current.set(Some((coords.x, coords.y)));
                                }
                            },
                            onmouseup: move |evt| {
                                let coords = evt.element_coordinates();
                                let x = coords.x;
                                let y = coords.y;
                                let (ann_type, geometry) = match mode {
                                    AnnotationMode::Highlight => {
                                        if let Some(start) = drag_start() {
                                            let rx = start.0.min(x); let ry = start.1.min(y);
                                            let rw = (start.0 - x).abs(); let rh = (start.1 - y).abs();
                                            if rw < 5.0 && rh < 5.0 {
                                                drag_start.set(None); drag_current.set(None); return;
                                            }
                                            (AnnotationType::Highlight, serde_json::json!({
                                                "x": rx, "y": ry, "width": rw, "height": rh,
                                                "page_width": width, "page_height": height,
                                            }))
                                        } else { return; }
                                    }
                                    AnnotationMode::Note => {
                                        (AnnotationType::Note, serde_json::json!({
                                            "x": x, "y": y, "width": 24.0, "height": 24.0,
                                            "page_width": width, "page_height": height,
                                        }))
                                    }
                                    AnnotationMode::None => return,
                                };
                                drag_start.set(None); drag_current.set(None);
                                let now = chrono::Utc::now();
                                let ann = Annotation {
                                    id: None, paper_id, page: page_index as i32, ann_type,
                                    color: color.clone(),
                                    content: if ann_type == AnnotationType::Note { Some(String::new()) } else { None },
                                    geometry, created_at: now, modified_at: now,
                                };
                                let db = db.clone();
                                spawn(async move {
                                    if let Ok(id) = crate::db::annotations::insert_annotation(db.conn(), &ann).await {
                                        let mut ann = ann;
                                        ann.id = Some(id);
                                        tabs.with_mut(|m| m.tab_mut().annotations.push(ann));
                                    }
                                });
                            },
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

// ── Annotation rendering ──────────────────────────────────────────

fn render_annotation(ann: &Annotation) -> Element {
    let x = ann.geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = ann.geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w = ann.geometry.get("width").and_then(|v| v.as_f64()).unwrap_or(24.0);
    let h = ann.geometry.get("height").and_then(|v| v.as_f64()).unwrap_or(24.0);
    let color = &ann.color;
    let ann_id = ann.id.unwrap_or(0);
    match ann.ann_type {
        AnnotationType::Highlight => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; background: {color}; opacity: 0.35; pointer-events: none; border-radius: 2px;" }
        },
        AnnotationType::Note => {
            let has_content = ann.content.as_ref().is_some_and(|c| !c.is_empty());
            let icon_bg = if has_content { color.as_str() } else { "#fbbf24" };
            rsx! {
                div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: 20px; height: 20px; background: {icon_bg}; border-radius: 4px; border: 1px solid rgba(0,0,0,0.2); cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 12px; pointer-events: auto;", title: "{ann.content.as_deref().unwrap_or(\"Empty note\")}", "N" }
            }
        }
        AnnotationType::Area => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; border: 2px solid {color}; pointer-events: none;" }
        },
    }
}

// ── Toolbar ───────────────────────────────────────────────────────

#[component]
fn PdfToolbar(page_count: u32, zoom: f32, tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut tools = use_context::<Signal<ViewerToolState>>();
    let render_ch = use_context::<RenderChannel>();
    let zoom_percent = (zoom * 100.0 / 1.5) as u32;

    let t = tools.read();
    let mode = t.annotation_mode;
    let current_color = t.annotation_color.clone();
    let show_panel = t.show_annotation_panel;
    drop(t);

    let ann_count = tabs.read().tab().annotations.len();

    let highlight_class = if mode == AnnotationMode::Highlight { "btn btn--ghost btn--ghost-active" } else { "btn btn--ghost" };
    let note_class = if mode == AnnotationMode::Note { "btn btn--ghost btn--ghost-active" } else { "btn btn--ghost" };
    let colors = vec!["#ffff00", "#ff6b6b", "#51cf66", "#339af0", "#cc5de8", "#ff922b"];

    rsx! {
        div { class: "pdf-toolbar",
            span { class: "toolbar-page-count", "{page_count} pages" }
            div { class: "toolbar-separator" }

            button {
                class: "{highlight_class}",
                onclick: move |_| {
                    tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Highlight { AnnotationMode::None } else { AnnotationMode::Highlight });
                },
                "Highlight"
            }
            button {
                class: "{note_class}",
                onclick: move |_| {
                    tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Note { AnnotationMode::None } else { AnnotationMode::Note });
                },
                "Note"
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

            div { class: "toolbar-spacer" }

            button {
                class: "btn btn--ghost",
                onclick: move |_| {
                    let render_tx = render_ch.sender();
                    tabs.with_mut(|m| m.tab_mut().nav.show_thumbnails = !m.tab().nav.show_thumbnails);
                    if tabs.read().tab().render.thumbnails.is_empty() {
                        spawn(async move {
                            let _ = crate::state::commands::load_thumbnails(&render_tx, &mut tabs, tab_id).await;
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

            div { class: "toolbar-separator" }

            button {
                class: "btn--icon",
                onclick: move |_| {
                    let new_zoom = (zoom - 0.3_f32).max(0.5);
                    let render_tx = render_ch.sender();
                    spawn(async move {
                        let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom).await;
                    });
                },
                "-"
            }
            span { class: "toolbar-zoom-value", "{zoom_percent}%" }
            button {
                class: "btn--icon",
                onclick: move |_| {
                    let new_zoom = (zoom + 0.3_f32).min(5.0);
                    let render_tx = render_ch.sender();
                    spawn(async move {
                        let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom).await;
                    });
                },
                "+"
            }
        }
    }
}

// ── Annotation panel ──────────────────────────────────────────────

#[component]
fn AnnotationPanel(tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let annotations = tabs.read().tab().annotations.clone();

    rsx! {
        div { class: "annotation-panel",
            div { class: "annotation-panel-header", "Annotations ({annotations.len()})" }
            if annotations.is_empty() {
                div { class: "annotation-panel-empty", "No annotations yet. Use the Highlight or Note tool to add annotations." }
            } else {
                div { class: "annotation-panel-list",
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
                                    class: "annotation-item",
                                    style: "border-left-color: {color};",
                                    div { class: "annotation-item-header",
                                        div { class: "annotation-item-meta",
                                            div { class: "annotation-color-dot", style: "background: {color};" }
                                            span { class: "annotation-type-label", "{type_label}" }
                                            span { class: "annotation-page-label", "p.{page + 1}" }
                                        }
                                        button {
                                            class: "btn--danger-sm",
                                            onclick: move |_| {
                                                let db = db_for_delete.clone();
                                                spawn(async move {
                                                    if let Ok(()) = crate::db::annotations::delete_annotation(db.conn(), ann_id).await {
                                                        tabs.with_mut(|m| m.tab_mut().annotations.retain(|a| a.id != Some(ann_id)));
                                                    }
                                                });
                                            },
                                            "x"
                                        }
                                    }
                                    if ann_type == AnnotationType::Note {
                                        if editing() {
                                            div { class: "annotation-edit-area",
                                                textarea { class: "textarea", value: "{edit_value}", oninput: move |evt| edit_value.set(evt.value()) }
                                                div { class: "annotation-edit-actions",
                                                    button {
                                                        class: "btn--save-sm",
                                                        onclick: move |_| {
                                                            let new_content = edit_value();
                                                            let db = db_for_save.clone();
                                                            let nc = new_content.clone();
                                                            spawn(async move {
                                                                let opt = if nc.is_empty() { None } else { Some(nc.as_str()) };
                                                                if let Ok(()) = crate::db::annotations::update_annotation_content(db.conn(), ann_id, opt).await {
                                                                    tabs.with_mut(|m| {
                                                                        if let Some(a) = m.tab_mut().annotations.iter_mut().find(|a| a.id == Some(ann_id)) {
                                                                            a.content = if new_content.is_empty() { None } else { Some(new_content.clone()) };
                                                                        }
                                                                    });
                                                                }
                                                                editing.set(false);
                                                            });
                                                        },
                                                        "Save"
                                                    }
                                                    button { class: "btn--cancel-sm", onclick: move |_| editing.set(false), "Cancel" }
                                                }
                                            }
                                        } else {
                                            div {
                                                class: "annotation-note-content",
                                                onclick: move |_| { edit_value.set(content.clone()); editing.set(true); },
                                                if content.is_empty() {
                                                    span { class: "annotation-note-empty", "Click to add note..." }
                                                } else { "{content}" }
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

// ── Search bar ────────────────────────────────────────────────────

#[component]
fn PdfSearchBar(tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mgr = tabs.read();
    let tab = mgr.tab();
    let query = tab.search.query.clone();
    let match_count = tab.search.matches.len();
    let current_idx = tab.search.current_index;
    drop(mgr);

    rsx! {
        div { class: "pdf-search-bar",
            input {
                class: "input input--sm pdf-search-input",
                r#type: "text",
                placeholder: "Search in PDF...",
                value: "{query}",
                oninput: move |evt| {
                    let new_query = evt.value();
                    tabs.with_mut(|m| {
                        let t = m.tab_mut();
                        t.search.query = new_query.clone();
                        let text_data: Vec<_> = t.render.text_data.values().cloned().collect();
                        t.search.matches = rotero_pdf::text_extract::search_in_text_data(&text_data, &new_query);
                        t.search.current_index = 0;
                    });
                },
                onkeydown: move |evt| {
                    if evt.key() == Key::Enter {
                        tabs.with_mut(|m| {
                            let t = m.tab_mut();
                            if !t.search.matches.is_empty() {
                                t.search.current_index = (t.search.current_index + 1) % t.search.matches.len();
                            }
                        });
                        let mgr = tabs.read();
                        if let Some(m) = mgr.tab().search.matches.get(mgr.tab().search.current_index) {
                            let page_idx = m.page_index;
                            drop(mgr);
                            spawn(async move {
                                let js = format!("let pages = document.querySelectorAll('.pdf-page-wrapper'); if (pages[{page_idx}]) {{ pages[{page_idx}].scrollIntoView({{ behavior: 'smooth', block: 'center' }}); }}");
                                let _ = document::eval(&js);
                            });
                        }
                    } else if evt.key() == Key::Escape {
                        tabs.with_mut(|m| {
                            let t = m.tab_mut();
                            t.search.visible = false;
                            t.search.query.clear();
                            t.search.matches.clear();
                            t.search.current_index = 0;
                        });
                    }
                },
                onmounted: move |evt| { let _ = evt.data().set_focus(true); },
            }
            if match_count > 0 {
                span { class: "pdf-search-count", "{current_idx + 1}/{match_count}" }
            }
            button { class: "btn--icon", onclick: move |_| {
                tabs.with_mut(|m| { let t = m.tab_mut(); if !t.search.matches.is_empty() { t.search.current_index = if t.search.current_index == 0 { t.search.matches.len() - 1 } else { t.search.current_index - 1 }; } });
            }, "\u{2191}" }
            button { class: "btn--icon", onclick: move |_| {
                tabs.with_mut(|m| { let t = m.tab_mut(); if !t.search.matches.is_empty() { t.search.current_index = (t.search.current_index + 1) % t.search.matches.len(); } });
            }, "\u{2193}" }
            button { class: "btn--icon", onclick: move |_| {
                tabs.with_mut(|m| { let t = m.tab_mut(); t.search.visible = false; t.search.query.clear(); t.search.matches.clear(); t.search.current_index = 0; });
            }, "\u{00d7}" }
        }
    }
}

// ── Thumbnail sidebar ─────────────────────────────────────────────

#[component]
fn ThumbnailSidebar() -> Element {
    let tabs = use_context::<Signal<PdfTabManager>>();
    let thumbnails = tabs.read().tab().render.thumbnails.clone();

    rsx! {
        div { class: "thumbnail-sidebar",
            for thumb in thumbnails.iter() {
                {
                    let page_idx = thumb.page_index;
                    let base64 = thumb.base64_data.clone();
                    let mime = thumb.mime;
                    let w = thumb.width;
                    let h = thumb.height;
                    let page_num = page_idx + 1;
                    rsx! {
                        div {
                            key: "thumb-{page_idx}", class: "thumbnail-item",
                            onclick: move |_| {
                                spawn(async move {
                                    let js = format!("let pages = document.querySelectorAll('.pdf-page-wrapper'); if (pages[{page_idx}]) {{ pages[{page_idx}].scrollIntoView({{ behavior: 'smooth', block: 'start' }}); }}");
                                    let _ = document::eval(&js);
                                });
                            },
                            img { class: "thumbnail-img", src: "data:{mime};base64,{base64}", width: "{w}", height: "{h}" }
                            span { class: "thumbnail-page-num", "{page_num}" }
                        }
                    }
                }
            }
        }
    }
}

// ── Outline panel ─────────────────────────────────────────────────

#[component]
fn OutlinePanel() -> Element {
    let tabs = use_context::<Signal<PdfTabManager>>();
    let outline = tabs.read().tab().nav.outline.clone();

    rsx! {
        div { class: "outline-panel",
            div { class: "outline-panel-header", "Table of Contents" }
            div { class: "outline-panel-list",
                for (idx, entry) in outline.iter().enumerate() {
                    {
                        let indent = entry.level as f64 * 16.0;
                        let page_idx = entry.page_index;
                        let title = entry.title.clone();
                        rsx! {
                            div {
                                key: "outline-{idx}", class: "outline-entry", style: "padding-left: {indent}px;",
                                onclick: move |_| {
                                    if let Some(pi) = page_idx {
                                        spawn(async move {
                                            let js = format!("let pages = document.querySelectorAll('.pdf-page-wrapper'); if (pages[{pi}]) {{ pages[{pi}].scrollIntoView({{ behavior: 'smooth', block: 'start' }}); }}");
                                            let _ = document::eval(&js);
                                        });
                                    }
                                },
                                "{title}"
                                if let Some(pi) = page_idx {
                                    span { class: "outline-page-num", " p.{pi + 1}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

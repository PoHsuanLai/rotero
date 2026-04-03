use dioxus::prelude::*;

use crate::app::RenderChannel;
use crate::db::Database;
use crate::state::app_state::{AnnotationMode, LibraryState, LibraryView, PdfTabManager, ViewerToolState, TabId};
use rotero_models::{Annotation, AnnotationType};
use super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};

/// Shared annotation context menu state: (ann_id, ann_type, page, color, content, x, y)
type AnnCtxState = Signal<Option<(i64, AnnotationType, i32, String, String, f64, f64)>>;

/// Convert a hex color like "#ff0000" to "rgba(r, g, b, alpha)".
fn hex_to_rgba(hex: &str, alpha: f32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        format!("rgba({r}, {g}, {b}, {alpha})")
    } else {
        format!("rgba(0, 100, 255, {alpha})")
    }
}

// ── Tab bar ───────────────────────────────────────────────────────

#[component]
pub fn PdfTabBar() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

    let mgr = tabs.read();
    let tab_info: Vec<(TabId, String, bool, Option<i64>)> = mgr.tabs.iter().map(|t| {
        (t.id, t.title.clone(), mgr.active_tab_id == Some(t.id), t.paper_id)
    }).collect();
    let tab_count = tab_info.len();
    drop(mgr);

    // Tab context menu state: (tab_id, paper_id, tab_index, x, y)
    let mut tab_ctx = use_signal(|| None::<(TabId, Option<i64>, usize, f64, f64)>);

    rsx! {
        div { class: "pdf-tab-bar",
            for (idx, (tab_id, title, is_active, paper_id)) in tab_info.iter().enumerate() {
                {
                    let tab_id = *tab_id;
                    let title = title.clone();
                    let is_active = *is_active;
                    let paper_id = *paper_id;
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
                            oncontextmenu: move |evt: Event<MouseData>| {
                                evt.prevent_default();
                                tab_ctx.set(Some((tab_id, paper_id, idx, evt.client_coordinates().x, evt.client_coordinates().y)));
                            },
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
                                        let cfg_dir = config.read().effective_library_path();
                                        let cfg_q = config.read().render_quality;
                                        tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                        let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, tab_id, &cfg_dir, cfg_q).await;
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
                                            let cfg_dir = config.read().effective_library_path();
                                            let cfg_q = config.read().render_quality;
                                            tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                            spawn(async move {
                                                let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, new_id, &cfg_dir, cfg_q).await;
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

            // Tab context menu
            if let Some((ctx_tab_id, ctx_paper_id, ctx_idx, mx, my)) = tab_ctx() {
                {
                    let has_tabs_to_right = ctx_idx + 1 < tab_count;
                    let has_other_tabs = tab_count > 1;

                    rsx! {
                        ContextMenu {
                            x: mx,
                            y: my,
                            on_close: move |_| {
                                tab_ctx.set(None);
                            },

                            ContextMenuItem {
                                label: "Close".to_string(),
                                icon: Some("bi-x-lg".to_string()),
                                on_click: move |_| {
                                    tabs.with_mut(|m| m.close_tab(ctx_tab_id));
                                    if tabs.read().tabs.is_empty() {
                                        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                                    }
                                    tab_ctx.set(None);
                                },
                            }

                            ContextMenuItem {
                                label: "Close other tabs".to_string(),
                                icon: Some("bi-x-circle".to_string()),
                                disabled: Some(!has_other_tabs),
                                on_click: move |_| {
                                    tabs.with_mut(|m| m.close_others(ctx_tab_id));
                                    tab_ctx.set(None);
                                },
                            }

                            ContextMenuItem {
                                label: "Close tabs to the right".to_string(),
                                icon: Some("bi-x-square".to_string()),
                                disabled: Some(!has_tabs_to_right),
                                on_click: move |_| {
                                    tabs.with_mut(|m| m.close_to_right(ctx_tab_id));
                                    tab_ctx.set(None);
                                },
                            }

                            if ctx_paper_id.is_some() {
                                ContextMenuSeparator {}

                                ContextMenuItem {
                                    label: "Show in library".to_string(),
                                    icon: Some("bi-collection".to_string()),
                                    on_click: move |_| {
                                        lib_state.with_mut(|s| {
                                            s.view = LibraryView::AllPapers;
                                            s.selected_paper_id = ctx_paper_id;
                                        });
                                        tab_ctx.set(None);
                                    },
                                }
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
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let db = use_context::<Database>();
    let mut is_loading = use_signal(|| false);
    use_context_provider::<AnnCtxState>(|| Signal::new(None));

    let mgr = tabs.read();
    let Some(tab) = mgr.active_tab() else {
        return rsx! {
            div { class: "pdf-viewer-empty", "Open a PDF to get started" }
        };
    };

    let tab_id = tab.id;
    let needs_render = tab.is_loading && tab.render.rendered_pages.is_empty();
    let is_initial_loading = needs_render;

    // Trigger render for tabs that need it (newly created or resumed)
    use_effect(move || {
        let needs = tabs.read().active_tab().map(|t| t.is_loading && t.render.rendered_pages.is_empty()).unwrap_or(false);
        if !needs { return; }
        let Some(tid) = tabs.read().active_tab_id else { return };
        let render_tx = render_ch.sender();
        let data_dir = config.read().effective_library_path();
        let db = db.clone();
        spawn(async move {
            if crate::state::commands::open_pdf(&render_tx, &mut tabs, tid, &data_dir, config.read().render_quality).await.is_ok() {
                // Load annotations if paper_id is set
                let paper_id = tabs.read().active_tab().and_then(|t| t.paper_id);
                if let Some(pid) = paper_id {
                    let mut anns = crate::db::annotations::list_annotations_for_paper(db.conn(), pid).await.unwrap_or_default();

                    // Extract annotations embedded in the PDF and import any new ones
                    let pdf_path = tabs.read().tab().pdf_path.clone();
                    let rendered_pages: Vec<(u32, u32)> = tabs.read().tab().render.rendered_pages
                        .iter()
                        .map(|p| (p.width, p.height))
                        .collect();

                    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                    if render_tx.send(crate::state::commands::RenderRequest::ExtractAnnotations {
                        pdf_path,
                        reply: reply_tx,
                    }).is_ok() {
                        let extract_result: Result<Result<Result<Vec<rotero_pdf::ExtractedAnnotation>, String>, _>, _> = tokio::task::spawn_blocking(move || reply_rx.recv()).await;
                        if let Ok(Ok(Ok(extracted))) = extract_result {
                            let now = chrono::Utc::now();
                            for ext in extracted {
                                // Deduplicate: skip if a DB annotation exists on same page with same type and similar position
                                let dominated = anns.iter().any(|a| {
                                    a.page == ext.page as i32
                                        && a.ann_type == ext.ann_type
                                        && {
                                            let ax = a.geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                            let ay = a.geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                            // Get rendered dims for this page to convert extracted coords
                                            let (rw, rh) = rendered_pages.get(ext.page as usize).copied().unwrap_or((1, 1));
                                            let sx = rw as f64 / ext.page_width_pts as f64;
                                            let sy = rh as f64 / ext.page_height_pts as f64;
                                            let ex = ext.rect_pts[0] as f64 * sx;
                                            let ey = (ext.page_height_pts as f64 - ext.rect_pts[3] as f64) * sy;
                                            (ax - ex).abs() < 10.0 && (ay - ey).abs() < 10.0
                                        }
                                });
                                if dominated { continue; }

                                // Convert PDF points to pixel coords
                                let (rw, rh) = rendered_pages.get(ext.page as usize).copied().unwrap_or((1, 1));
                                let sx = rw as f32 / ext.page_width_pts;
                                let sy = rh as f32 / ext.page_height_pts;
                                let x = ext.rect_pts[0] * sx;
                                let y = (ext.page_height_pts - ext.rect_pts[3]) * sy;
                                let w = (ext.rect_pts[2] - ext.rect_pts[0]) * sx;
                                let h = (ext.rect_pts[3] - ext.rect_pts[1]) * sy;

                                let geometry = serde_json::json!({
                                    "x": x, "y": y, "width": w, "height": h,
                                    "page_width": rw, "page_height": rh,
                                });

                                let ann = Annotation {
                                    id: None,
                                    paper_id: pid,
                                    page: ext.page as i32,
                                    ann_type: ext.ann_type,
                                    color: ext.color,
                                    content: ext.content,
                                    geometry,
                                    created_at: now,
                                    modified_at: now,
                                };
                                if let Ok(id) = crate::db::annotations::insert_annotation(db.conn(), &ann).await {
                                    let mut ann = ann;
                                    ann.id = Some(id);
                                    anns.push(ann);
                                }
                            }
                        }
                    }

                    tabs.with_mut(|m| {
                        if let Some(t) = m.tabs.iter_mut().find(|t| t.id == tid) {
                            t.annotations = anns;
                        }
                    });
                }
            }
        });
    });
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
                            let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom, config.read().render_quality).await;
                        });
                    }
                    Key::Character(ref c) if c == "-" => {
                        let new_zoom = (zoom - 0.3_f32).max(0.5);
                        let render_tx = render_ch.sender();
                        spawn(async move {
                            let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom, config.read().render_quality).await;
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
                        // Handled by global shortcut in keybindings.rs
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
                        if is_loading() { return; }
                        let (start, has_more_now, tid) = {
                            let mgr = tabs.read();
                            if let Some(t) = mgr.active_tab() {
                                let rendered = t.render.rendered_pages.len() as u32;
                                (rendered, rendered < t.page_count, t.id)
                            } else { return; }
                        };
                        if !has_more_now { return; }

                        let render_tx = render_ch.sender();
                        let count = batch_size;
                        let quality = config.read().render_quality;
                        is_loading.set(true);
                        spawn(async move {
                            let _ = crate::state::commands::render_more_pages(
                                &render_tx, &mut tabs, tid, start, count, quality,
                            ).await;
                            is_loading.set(false);
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

            // Annotation context menu (shared between page overlays and panel)
            AnnotationContextMenu {}
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
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let ann_ctx = use_context::<AnnCtxState>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

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

    let selection_color = {
        let hex = &config.read().selection_color;
        hex_to_rgba(hex, 0.3)
    };

    let t = tools.read();
    let mode = t.annotation_mode;
    let color = t.annotation_color.clone();
    drop(t);

    let cursor = match mode {
        AnnotationMode::Highlight | AnnotationMode::Underline => "crosshair",
        AnnotationMode::Note => "cell",
        AnnotationMode::Ink => "crosshair",
        AnnotationMode::Text => "text",
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
                id: "text-layer-{page_index}",
                style: "width: {width}px; height: {height}px; --selection-color: {selection_color};",
                onmounted: move |_| {
                    spawn(async move {
                        let js = format!(r#"
                            (function() {{
                                let layer = document.getElementById('text-layer-{page_index}');
                                if (!layer) return;

                                // --- Text scaling ---
                                let spans = layer.querySelectorAll('span[data-target-w]');
                                let canvas = document.createElement('canvas');
                                let ctx = canvas.getContext('2d');

                                // Detect browser minimum font size
                                let probe = document.createElement('div');
                                probe.style.fontSize = '1px';
                                probe.style.lineHeight = '1';
                                probe.style.position = 'absolute';
                                probe.style.opacity = '0';
                                probe.textContent = 'X';
                                document.body.appendChild(probe);
                                let minFs = probe.getBoundingClientRect().height;
                                probe.remove();

                                for (let span of spans) {{
                                    if (span.textContent.length <= 1) continue;

                                    let targetW = parseFloat(span.dataset.targetW);
                                    let fontSize = parseFloat(span.style.fontSize);
                                    let fontStyle = span.dataset.fontStyle || 'normal';
                                    let fontWeight = span.dataset.fontWeight || 'normal';
                                    let fontFamily = span.style.fontFamily || 'sans-serif';

                                    let scaledFontSize = fontSize;
                                    if (minFs > 1) {{
                                        scaledFontSize = fontSize * minFs;
                                        span.style.fontSize = scaledFontSize + 'px';
                                    }}

                                    ctx.font = fontStyle + ' ' + fontWeight + ' ' + scaledFontSize + 'px ' + fontFamily;
                                    let measured = ctx.measureText(span.textContent).width;

                                    let transform = '';
                                    if (minFs > 1) {{
                                        transform = 'scale(' + (1 / minFs) + ')';
                                    }}
                                    if (measured > 0 && targetW > 0) {{
                                        let sx = targetW / measured;
                                        transform = 'scaleX(' + sx + ') ' + transform;
                                    }}
                                    if (transform) span.style.transform = transform;
                                }}

                                // --- Custom selection overlay ---
                                // Hide native ::selection and draw our own per-line merged rects
                                let selColor = getComputedStyle(layer).getPropertyValue('--selection-color').trim()
                                    || 'rgba(0, 100, 255, 0.3)';

                                document.addEventListener('selectionchange', function() {{
                                    // Remove old highlights
                                    layer.querySelectorAll('.selection-highlight').forEach(function(el) {{ el.remove(); }});

                                    let sel = document.getSelection();
                                    if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return;

                                    let range = sel.getRangeAt(0);
                                    if (!range.intersectsNode(layer)) return;

                                    // Collect selected spans and group by line (similar y)
                                    let selected = [];
                                    for (let span of spans) {{
                                        if (sel.containsNode(span, true)) {{
                                            let x = parseFloat(span.dataset.segX);
                                            let y = parseFloat(span.dataset.segY);
                                            let w = parseFloat(span.dataset.targetW);
                                            let h = parseFloat(span.dataset.segH);
                                            if (!isNaN(x) && !isNaN(y) && !isNaN(w) && !isNaN(h)) {{
                                                selected.push({{ x: x, y: y, w: w, h: h }});
                                            }}
                                        }}
                                    }}

                                    if (!selected.length) return;

                                    // Sort by y then x
                                    selected.sort(function(a, b) {{ return a.y - b.y || a.x - b.x; }});

                                    // Group into lines by y proximity
                                    let lines = [];
                                    let curLine = [selected[0]];
                                    for (let i = 1; i < selected.length; i++) {{
                                        let s = selected[i];
                                        let tol = s.h * 0.5;
                                        if (Math.abs(s.y - curLine[0].y) < tol) {{
                                            curLine.push(s);
                                        }} else {{
                                            lines.push(curLine);
                                            curLine = [s];
                                        }}
                                    }}
                                    lines.push(curLine);

                                    // Draw one merged rect per line
                                    for (let line of lines) {{
                                        let minX = Infinity, minY = Infinity, maxR = -Infinity, maxB = -Infinity;
                                        for (let s of line) {{
                                            if (s.x < minX) minX = s.x;
                                            if (s.y < minY) minY = s.y;
                                            if (s.x + s.w > maxR) maxR = s.x + s.w;
                                            if (s.y + s.h > maxB) maxB = s.y + s.h;
                                        }}
                                        let div = document.createElement('div');
                                        div.className = 'selection-highlight';
                                        div.style.left = minX + 'px';
                                        div.style.top = minY + 'px';
                                        div.style.width = (maxR - minX) + 'px';
                                        div.style.height = (maxB - minY) + 'px';
                                        div.style.background = selColor;
                                        layer.appendChild(div);
                                    }}
                                }});

                                // Copy handler: normalize text
                                layer.addEventListener('copy', function(evt) {{
                                    let sel = document.getSelection();
                                    if (sel) {{
                                        let text = sel.toString().normalize('NFC').replace(/\0/g, '');
                                        evt.clipboardData.setData('text/plain', text);
                                        evt.preventDefault();
                                    }}
                                }});
                            }})()
                        "#);
                        let _ = document::eval(&js);
                    });
                },
                {
                    let lines = rotero_pdf::group_into_lines(&text_segments);
                    rsx! {
                        for (line_idx, line) in lines.iter().enumerate() {
                            if line_idx > 0 {
                                br { key: "br-{page_index}-{line_idx}" }
                            }
                            for &seg_idx in line.iter() {
                                {
                                    let seg = &text_segments[seg_idx];
                                    rsx! {
                                        span {
                                            key: "text-{page_index}-{seg_idx}",
                                            "data-target-w": "{seg.width}",
                                            "data-seg-x": "{seg.x}",
                                            "data-seg-y": "{seg.y}",
                                            "data-seg-h": "{seg.height}",
                                            "data-font-weight": "{seg.font_weight}",
                                            "data-font-style": "{seg.font_style}",
                                            style: "left: {seg.x}px; top: {seg.y}px; font-size: {seg.font_size}px; font-family: {seg.font_family}; font-weight: {seg.font_weight}; font-style: {seg.font_style};",
                                            "{seg.text}"
                                        }
                                    }
                                }
                            }
                        }
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
                {render_annotation(ann, ann_ctx)}
            }

            if mode != AnnotationMode::None {
                {
                    let mut drag_start = use_signal(|| None::<(f64, f64)>);
                    let mut drag_current = use_signal(|| None::<(f64, f64)>);
                    let mut ink_points = use_signal(Vec::<f64>::new);
                    let drag_rect = if mode == AnnotationMode::Highlight || mode == AnnotationMode::Underline {
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
                                if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                                if mode == AnnotationMode::Highlight || mode == AnnotationMode::Underline {
                                    let coords = evt.element_coordinates();
                                    drag_start.set(Some((coords.x, coords.y)));
                                    drag_current.set(Some((coords.x, coords.y)));
                                }
                                if mode == AnnotationMode::Ink {
                                    let coords = evt.element_coordinates();
                                    drag_start.set(Some((coords.x, coords.y)));
                                    ink_points.with_mut(|pts| {
                                        pts.clear();
                                        pts.push(coords.x);
                                        pts.push(coords.y);
                                    });
                                }
                            },
                            onmousemove: move |evt| {
                                if (mode == AnnotationMode::Highlight || mode == AnnotationMode::Underline) && drag_start().is_some() {
                                    let coords = evt.element_coordinates();
                                    drag_current.set(Some((coords.x, coords.y)));
                                }
                                if mode == AnnotationMode::Ink && drag_start().is_some() {
                                    let coords = evt.element_coordinates();
                                    ink_points.with_mut(|pts| {
                                        pts.push(coords.x);
                                        pts.push(coords.y);
                                    });
                                }
                            },
                            onmouseup: move |evt| {
                                if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                                let coords = evt.element_coordinates();
                                let x = coords.x;
                                let y = coords.y;
                                let (ann_type, geometry) = match mode {
                                    AnnotationMode::Highlight | AnnotationMode::Underline => {
                                        let at = if mode == AnnotationMode::Highlight { AnnotationType::Highlight } else { AnnotationType::Underline };
                                        if let Some(start) = drag_start() {
                                            let rx = start.0.min(x); let ry = start.1.min(y);
                                            let rw = (start.0 - x).abs(); let rh = (start.1 - y).abs();
                                            if rw < 5.0 && rh < 5.0 {
                                                drag_start.set(None); drag_current.set(None); return;
                                            }
                                            (at, serde_json::json!({
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
                                    AnnotationMode::Ink => {
                                        let pts = ink_points.read().clone();
                                        ink_points.with_mut(|p| p.clear());
                                        if pts.len() < 4 {
                                            drag_start.set(None); return;
                                        }
                                        // Compute bounding box
                                        let mut min_x = f64::MAX; let mut min_y = f64::MAX;
                                        let mut max_x = f64::MIN; let mut max_y = f64::MIN;
                                        for i in (0..pts.len()).step_by(2) {
                                            let px = pts[i]; let py = pts[i + 1];
                                            if px < min_x { min_x = px; }
                                            if py < min_y { min_y = py; }
                                            if px > max_x { max_x = px; }
                                            if py > max_y { max_y = py; }
                                        }
                                        (AnnotationType::Ink, serde_json::json!({
                                            "x": min_x, "y": min_y,
                                            "width": max_x - min_x, "height": max_y - min_y,
                                            "page_width": width, "page_height": height,
                                            "points": [pts],
                                        }))
                                    }
                                    AnnotationMode::Text => {
                                        (AnnotationType::Text, serde_json::json!({
                                            "x": x, "y": y, "width": 150.0, "height": 20.0,
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
                                    content: if matches!(ann_type, AnnotationType::Note | AnnotationType::Text) { Some(String::new()) } else { None },
                                    geometry, created_at: now, modified_at: now,
                                };
                                let db = db.clone();
                                spawn(async move {
                                    if let Ok(id) = crate::db::annotations::insert_annotation(db.conn(), &ann).await {
                                        let mut ann = ann;
                                        ann.id = Some(id);
                                        undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Create(ann.clone())));
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

fn render_annotation(ann: &Annotation, mut ann_ctx: AnnCtxState) -> Element {
    let x = ann.geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let y = ann.geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let w = ann.geometry.get("width").and_then(|v| v.as_f64()).unwrap_or(24.0);
    let h = ann.geometry.get("height").and_then(|v| v.as_f64()).unwrap_or(24.0);
    let color = ann.color.clone();
    let ann_id = ann.id.unwrap_or(0);
    let ann_type = ann.ann_type;
    let page = ann.page;
    let content = ann.content.clone().unwrap_or_default();
    let color_for_ctx = color.clone();

    let on_context = move |evt: Event<MouseData>| {
        evt.prevent_default();
        ann_ctx.set(Some((ann_id, ann_type, page, color_for_ctx.clone(), content.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
    };

    match ann.ann_type {
        AnnotationType::Highlight => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; background: {color}; opacity: 0.35; pointer-events: auto; border-radius: 2px; z-index: 3;", oncontextmenu: on_context }
        },
        AnnotationType::Note => {
            let icon_bg = ann.color.clone();
            let title = ann.content.as_deref().unwrap_or("Empty note").to_string();
            rsx! {
                div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: 20px; height: 20px; background: {icon_bg}; border-radius: 4px; border: 1px solid rgba(0,0,0,0.2); cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 12px; pointer-events: auto; z-index: 3;", title: "{title}", oncontextmenu: on_context, "N" }
            }
        }
        AnnotationType::Area => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; border: 2px solid {color}; pointer-events: auto; z-index: 3;", oncontextmenu: on_context }
        },
        AnnotationType::Underline => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; border-bottom: 2px solid {color}; pointer-events: auto; z-index: 3;", oncontextmenu: on_context }
        },
        AnnotationType::Ink => {
            // Build SVG path from stored points
            let points = ann.geometry.get("points")
                .and_then(|v| v.as_array())
                .and_then(|strokes| strokes.first())
                .and_then(|s| s.as_array());
            let path_d = if let Some(pts) = points {
                let coords: Vec<f64> = pts.iter().filter_map(|v| v.as_f64()).collect();
                if coords.len() >= 4 {
                    let mut d = format!("M{},{}", coords[0] - x, coords[1] - y);
                    for i in (2..coords.len()).step_by(2) {
                        d.push_str(&format!(" L{},{}", coords[i] - x, coords[i + 1] - y));
                    }
                    d
                } else { String::new() }
            } else { String::new() };
            rsx! {
                svg {
                    key: "ann-{ann_id}",
                    style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; pointer-events: auto; z-index: 3; overflow: visible;",
                    oncontextmenu: on_context,
                    path { d: "{path_d}", stroke: "{color}", stroke_width: "2", fill: "none", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
        }
        AnnotationType::Text => {
            let text = ann.content.as_deref().unwrap_or("").to_string();
            rsx! {
                div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; min-width: 40px; padding: 2px 4px; background: rgba(255,255,200,0.9); border: 1px solid {color}; font-size: 12px; pointer-events: auto; z-index: 3; white-space: pre-wrap; color: #333;", oncontextmenu: on_context, "{text}" }
            }
        }
    }
}

// ── Toolbar ───────────────────────────────────────────────────────

#[component]
fn PdfToolbar(page_count: u32, zoom: f32, tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut tools = use_context::<Signal<ViewerToolState>>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
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
        if mode == m { "btn btn--ghost btn--ghost-active" } else { "btn btn--ghost" }
    };
    let highlight_class = btn(AnnotationMode::Highlight);
    let underline_class = btn(AnnotationMode::Underline);
    let note_class = btn(AnnotationMode::Note);
    let ink_class = btn(AnnotationMode::Ink);
    let text_class = btn(AnnotationMode::Text);
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
                span { class: "bi bi-highlighter" }
            }
            button {
                class: "{underline_class}",
                onclick: move |_| {
                    tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Underline { AnnotationMode::None } else { AnnotationMode::Underline });
                },
                span { class: "bi bi-type-underline" }
            }
            button {
                class: "{note_class}",
                onclick: move |_| {
                    tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Note { AnnotationMode::None } else { AnnotationMode::Note });
                },
                span { class: "bi bi-sticky" }
            }
            button {
                class: "{ink_class}",
                onclick: move |_| {
                    tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Ink { AnnotationMode::None } else { AnnotationMode::Ink });
                },
                span { class: "bi bi-pencil" }
            }
            button {
                class: "{text_class}",
                onclick: move |_| {
                    tools.with_mut(|t| t.annotation_mode = if t.annotation_mode == AnnotationMode::Text { AnnotationMode::None } else { AnnotationMode::Text });
                },
                span { class: "bi bi-fonts" }
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

            div { class: "toolbar-spacer" }

            button {
                class: "btn btn--ghost",
                onclick: move |_| {
                    let render_tx = render_ch.sender();
                    tabs.with_mut(|m| m.tab_mut().nav.show_thumbnails = !m.tab().nav.show_thumbnails);
                    if tabs.read().tab().render.thumbnails.is_empty() {
                        spawn(async move {
                            let _ = crate::state::commands::load_thumbnails(&render_tx, &mut tabs, tab_id, config.read().thumbnail_quality).await;
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

                        let file = super::save_file(&["pdf"], "Export PDF with Annotations", &default_name);

                        if let Some(output_path) = file {
                            let render_tx = render_ch.sender();
                            spawn(async move {
                                let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                                if render_tx.send(crate::state::commands::RenderRequest::GetPageDimensions {
                                    pdf_path: pdf_path.clone(),
                                    reply: reply_tx,
                                }).is_err() {
                                    eprintln!("Failed to send GetPageDimensions request");
                                    return;
                                }
                                let dims = match tokio::task::spawn_blocking(move || reply_rx.recv()).await {
                                    Ok(Ok(Ok(d))) => d,
                                    _ => {
                                        eprintln!("Failed to get page dimensions");
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
                                    Err(e) => eprintln!("Failed to export annotated PDF: {e}"),
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
                    let render_tx = render_ch.sender();
                    spawn(async move {
                        let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom, config.read().render_quality).await;
                    });
                },
                span { class: "bi bi-zoom-out" }
            }
            span { class: "toolbar-zoom-value", "{zoom_percent}%" }
            button {
                class: "btn btn--ghost btn--sm toolbar-zoom-btn",
                onclick: move |_| {
                    let new_zoom = (zoom + 0.3_f32).min(5.0);
                    let render_tx = render_ch.sender();
                    spawn(async move {
                        let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom, config.read().render_quality).await;
                    });
                },
                span { class: "bi bi-zoom-in" }
            }
        }
    }
}

// ── Annotation panel ──────────────────────────────────────────────

#[component]
fn AnnotationPanel(tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let mut ann_ctx = use_context::<AnnCtxState>();
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
                                AnnotationType::Underline => "Underline",
                                AnnotationType::Ink => "Ink",
                                AnnotationType::Text => "Text",
                            };
                            let ctx_color = color.clone();
                            let ctx_content = content.clone();
                            rsx! {
                                div {
                                    key: "panel-ann-{ann_id}",
                                    class: "annotation-item",
                                    style: "border-left-color: {color};",
                                    oncontextmenu: move |evt: Event<MouseData>| {
                                        evt.prevent_default();
                                        ann_ctx.set(Some((ann_id, ann_type, page, ctx_color.clone(), ctx_content.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                                    },
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
                                                let deleted_ann = tabs.read().tab().annotations.iter().find(|a| a.id == Some(ann_id)).cloned();
                                                spawn(async move {
                                                    if let Ok(()) = crate::db::annotations::delete_annotation(db.conn(), ann_id).await {
                                                        if let Some(ann) = deleted_ann {
                                                            undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Delete(ann)));
                                                        }
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
                                                            let old_content = content.clone();
                                                            let db = db_for_save.clone();
                                                            let nc = new_content.clone();
                                                            spawn(async move {
                                                                let opt = if nc.is_empty() { None } else { Some(nc.as_str()) };
                                                                if let Ok(()) = crate::db::annotations::update_annotation_content(db.conn(), ann_id, opt).await {
                                                                    let old = if old_content.is_empty() { None } else { Some(old_content) };
                                                                    let new = if new_content.is_empty() { None } else { Some(new_content.clone()) };
                                                                    undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::UpdateContent { id: ann_id, old, new }));
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

// ── Annotation context menu ───────────────────────────────────────

#[component]
fn AnnotationContextMenu() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let mut ann_ctx = use_context::<AnnCtxState>();

    let Some((ctx_ann_id, ctx_type, ctx_page, ctx_old_color, ctx_content, mx, my)) = ann_ctx() else {
        return rsx! {};
    };

    let db_color = db.clone();
    let db_delete = db.clone();
    let colors = vec![
        ("#ffff00", "Yellow"),
        ("#ff6b6b", "Red"),
        ("#51cf66", "Green"),
        ("#339af0", "Blue"),
        ("#cc5de8", "Purple"),
        ("#ff922b", "Orange"),
    ];

    rsx! {
        ContextMenu {
            x: mx,
            y: my,
            on_close: move |_| {
                ann_ctx.set(None);
            },

            ContextMenuItem {
                label: format!("Go to page {}", ctx_page + 1),
                icon: Some("bi-arrow-right-circle".to_string()),
                on_click: move |_| {
                    let js = format!(
                        "let el = document.getElementById('pdf-page-{}'); if (el) el.scrollIntoView({{behavior: 'smooth'}})",
                        ctx_page
                    );
                    let _ = document::eval(&js);
                    ann_ctx.set(None);
                },
            }

            if ctx_type == AnnotationType::Note {
                ContextMenuItem {
                    label: "Edit note".to_string(),
                    icon: Some("bi-pencil".to_string()),
                    on_click: move |_| {
                        ann_ctx.set(None);
                    },
                }
            }

            if ctx_type == AnnotationType::Highlight && !ctx_content.is_empty() {
                {
                    let text = ctx_content.clone();
                    rsx! {
                        ContextMenuItem {
                            label: "Copy text".to_string(),
                            icon: Some("bi-clipboard".to_string()),
                            on_click: move |_| {
                                let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&text).unwrap_or_default());
                                let _ = document::eval(&js);
                                ann_ctx.set(None);
                            },
                        }
                    }
                }
            }

            div { class: "context-menu-item",
                i { class: "context-menu-icon bi bi-palette" }
                span { class: "context-menu-label", "Color" }
                div { class: "context-menu-colors",
                    for (color, _label) in colors.iter() {
                        {
                            let color = color.to_string();
                            let color_for_click = color.clone();
                            let db_swatch = db_color.clone();
                            let old_color_for_swatch = ctx_old_color.clone();
                            rsx! {
                                span {
                                    class: "context-menu-color-swatch",
                                    style: "background: {color};",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        let c = color_for_click.clone();
                                        let old_c = old_color_for_swatch.clone();
                                        let db = db_swatch.clone();
                                        spawn(async move {
                                            if let Ok(()) = crate::db::annotations::update_annotation_color(db.conn(), ctx_ann_id, &c).await {
                                                undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::UpdateColor { id: ctx_ann_id, old: old_c, new: c.clone() }));
                                                tabs.with_mut(|m| {
                                                    if let Some(a) = m.tab_mut().annotations.iter_mut().find(|a| a.id == Some(ctx_ann_id)) {
                                                        a.color = c;
                                                    }
                                                });
                                            }
                                            ann_ctx.set(None);
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }

            ContextMenuSeparator {}

            ContextMenuItem {
                label: "Delete".to_string(),
                icon: Some("bi-trash".to_string()),
                danger: Some(true),
                on_click: move |_| {
                    let db = db_delete.clone();
                    let deleted_ann = tabs.read().tab().annotations.iter().find(|a| a.id == Some(ctx_ann_id)).cloned();
                    spawn(async move {
                        if let Ok(()) = crate::db::annotations::delete_annotation(db.conn(), ctx_ann_id).await {
                            if let Some(ann) = deleted_ann {
                                undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Delete(ann)));
                            }
                            tabs.with_mut(|m| m.tab_mut().annotations.retain(|a| a.id != Some(ctx_ann_id)));
                        }
                    });
                    ann_ctx.set(None);
                },
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

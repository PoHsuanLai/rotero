use dioxus::prelude::*;

use super::annotation_panel::{AnnotationContextMenu, AnnotationPanel};
use super::navigation::{OutlinePanel, ThumbnailSidebar};
use super::page_overlay::PdfPageWithOverlay;
use super::search_bar::PdfSearchBar;
use super::toolbar::PdfToolbar;
use super::AnnCtxState;
use crate::app::RenderChannel;
use crate::state::app_state::{PdfTabManager, TabId, ViewerToolState};
use rotero_db::Database;

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
        let needs = tabs
            .read()
            .active_tab()
            .map(|t| t.is_loading && t.render.rendered_pages.is_empty())
            .unwrap_or(false);
        if !needs {
            return;
        }
        let Some(tid) = tabs.read().active_tab_id else {
            return;
        };
        let render_tx = render_ch.sender();
        let data_dir = config.read().effective_library_path();
        let db = db.clone();
        spawn(async move {
            if crate::state::commands::open_pdf(
                &render_tx, &mut tabs, tid, &data_dir,
            )
            .await
            .is_ok()
            {
                // Load annotations if paper_id is set
                let paper_id = tabs.read().active_tab().and_then(|t| t.paper_id.clone());
                if let Some(ref pid) = paper_id {
                    let mut anns =
                        rotero_db::annotations::list_annotations_for_paper(db.conn(), pid)
                            .await
                            .unwrap_or_default();

                    // Extract annotations embedded in the PDF and import any new ones
                    let pdf_path = tabs.read().tab().pdf_path.clone();
                    let rendered_pages: Vec<(u32, u32)> = tabs
                        .read()
                        .tab()
                        .render
                        .rendered_pages
                        .iter()
                        .map(|p| (p.width, p.height))
                        .collect();

                    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
                    if render_tx
                        .send(crate::state::commands::RenderRequest::ExtractAnnotations {
                            pdf_path,
                            reply: reply_tx,
                        })
                        .is_ok()
                    {
                        let extract_result: Result<
                            Result<Result<Vec<rotero_pdf::ExtractedAnnotation>, String>, _>,
                            _,
                        > = tokio::task::spawn_blocking(move || reply_rx.recv()).await;
                        if let Ok(Ok(Ok(extracted))) = extract_result {
                            let now = chrono::Utc::now();
                            for ext in extracted {
                                // Deduplicate: skip if a DB annotation exists on same page with same type and similar position
                                let dominated = anns.iter().any(|a| {
                                    a.page == ext.page as i32 && a.ann_type == ext.ann_type && {
                                        let ax = a
                                            .geometry
                                            .get("x")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0);
                                        let ay = a
                                            .geometry
                                            .get("y")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0);
                                        // Get rendered dims for this page to convert extracted coords
                                        let (rw, rh) = rendered_pages
                                            .get(ext.page as usize)
                                            .copied()
                                            .unwrap_or((1, 1));
                                        let sx = rw as f64 / ext.page_width_pts as f64;
                                        let sy = rh as f64 / ext.page_height_pts as f64;
                                        let ex = ext.rect_pts[0] as f64 * sx;
                                        let ey = (ext.page_height_pts as f64
                                            - ext.rect_pts[3] as f64)
                                            * sy;
                                        (ax - ex).abs() < 10.0 && (ay - ey).abs() < 10.0
                                    }
                                });
                                if dominated {
                                    continue;
                                }

                                // Convert PDF points to pixel coords
                                let (rw, rh) = rendered_pages
                                    .get(ext.page as usize)
                                    .copied()
                                    .unwrap_or((1, 1));
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

                                let ann = rotero_models::Annotation {
                                    id: None,
                                    paper_id: pid.clone(),
                                    page: ext.page as i32,
                                    ann_type: ext.ann_type,
                                    color: ext.color,
                                    content: ext.content,
                                    geometry,
                                    created_at: now,
                                    modified_at: now,
                                };
                                if let Ok(id) =
                                    rotero_db::annotations::insert_annotation(db.conn(), &ann).await
                                {
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
    let rendered_count = tab.rendered_count();
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
                drop(evt.data().set_focus(true));
            },
            onkeydown: move |evt| {
                let key = evt.key();
                match key {
                    Key::Character(ref c) if c == "+" || c == "=" => {
                        let new_zoom = (zoom + 0.3_f32).min(5.0);
                        let render_tx = render_ch.sender();
                        let data_dir = config.read().effective_library_path();
                        spawn(async move {
                            let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom, &data_dir).await;
                        });
                    }
                    Key::Character(ref c) if c == "-" => {
                        let new_zoom = (zoom - 0.3_f32).max(0.5);
                        let render_tx = render_ch.sender();
                        let data_dir = config.read().effective_library_path();
                        spawn(async move {
                            let _ = crate::state::commands::set_zoom(&render_tx, &mut tabs, tab_id, new_zoom, &data_dir).await;
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
                    onmounted: move |_| {
                        // Install drag-to-pan for mouse (middle-click or space+drag)
                        // and touch (two-finger pan) on the scroll container.
                        spawn(async move {
                            let _ = document::eval(r#"
                            (function() {
                                let el = document.getElementById('pdf-pages-container');
                                if (!el || el.__panInstalled) return;
                                el.__panInstalled = true;

                                let isPanning = false;
                                let startX = 0, startY = 0;
                                let scrollLeft = 0, scrollTop = 0;

                                // Mouse: middle-click drag or left-click on empty area
                                el.addEventListener('mousedown', function(e) {
                                    // Middle mouse button, or left button with space key held
                                    if (e.button === 1 || (e.button === 0 && e.target === el)) {
                                        isPanning = true;
                                        startX = e.clientX;
                                        startY = e.clientY;
                                        scrollLeft = el.scrollLeft;
                                        scrollTop = el.scrollTop;
                                        el.classList.add('panning');
                                        e.preventDefault();
                                    }
                                });

                                window.addEventListener('mousemove', function(e) {
                                    if (!isPanning) return;
                                    el.scrollLeft = scrollLeft - (e.clientX - startX);
                                    el.scrollTop = scrollTop - (e.clientY - startY);
                                });

                                window.addEventListener('mouseup', function(e) {
                                    if (isPanning) {
                                        isPanning = false;
                                        el.classList.remove('panning');
                                    }
                                });

                                // Touch: two-finger pan
                                let touchStartX = 0, touchStartY = 0;
                                let touchScrollLeft = 0, touchScrollTop = 0;
                                let isTouchPanning = false;

                                el.addEventListener('touchstart', function(e) {
                                    if (e.touches.length === 2) {
                                        isTouchPanning = true;
                                        let mid = midpoint(e.touches);
                                        touchStartX = mid.x;
                                        touchStartY = mid.y;
                                        touchScrollLeft = el.scrollLeft;
                                        touchScrollTop = el.scrollTop;
                                    }
                                }, { passive: true });

                                el.addEventListener('touchmove', function(e) {
                                    if (!isTouchPanning || e.touches.length < 2) return;
                                    let mid = midpoint(e.touches);
                                    el.scrollLeft = touchScrollLeft - (mid.x - touchStartX);
                                    el.scrollTop = touchScrollTop - (mid.y - touchStartY);
                                }, { passive: true });

                                el.addEventListener('touchend', function(e) {
                                    if (e.touches.length < 2) isTouchPanning = false;
                                }, { passive: true });

                                function midpoint(touches) {
                                    return {
                                        x: (touches[0].clientX + touches[1].clientX) / 2,
                                        y: (touches[0].clientY + touches[1].clientY) / 2
                                    };
                                }
                            })();
                            "#);
                        });
                    },
                    onscroll: move |_| {
                        if is_loading() { return; }
                        let (start, has_more_now, tid) = {
                            let mgr = tabs.read();
                            if let Some(t) = mgr.active_tab() {
                                let rendered = t.rendered_count();
                                (rendered, rendered < t.page_count, t.id)
                            } else { return; }
                        };
                        if !has_more_now { return; }

                        is_loading.set(true);
                        spawn(async move {
                            let mut eval = document::eval(
                                "(function() { let el = document.getElementById('pdf-pages-container'); \
                                 if (!el) return 0.0; \
                                 return (el.scrollHeight - el.scrollTop - el.clientHeight) / el.clientHeight; })()"
                            );
                            let remaining_viewports = eval.recv::<f64>().await.unwrap_or(0.0);
                            if remaining_viewports > 2.0 {
                                is_loading.set(false);
                                return;
                            }

                            let render_tx = render_ch.sender();
                            let count = batch_size;
                            let data_dir = config.read().effective_library_path();
                            let _ = crate::state::commands::render_more_pages(
                                &render_tx, &mut tabs, tid, start, count, &data_dir,
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

use dioxus::prelude::*;

use super::AnnCtxState;
use super::annotation_panel::{AnnotationContextMenu, AnnotationPanel};
use super::navigation::{OutlinePanel, ThumbnailSidebar};
use super::page_overlay::PdfPageWithOverlay;
use super::search_bar::PdfSearchBar;
use super::toolbar::PdfToolbar;
use crate::app::RenderChannel;
use crate::state::app_state::{PdfTabManager, ViewerToolState};
use rotero_db::Database;

#[component]
pub fn PdfViewer() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let tools = use_context::<Signal<ViewerToolState>>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let db = use_context::<Database>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();
    use_context_provider::<AnnCtxState>(|| Signal::new(None));
    // Guards the scroll-driven render window against re-entrant scroll events.
    let mut window_loading = use_signal(|| false);

    let mgr = tabs.read();
    let Some(tab) = mgr.active_tab() else {
        return rsx! {
            div { class: "pdf-viewer-empty", "Open a PDF to get started" }
        };
    };

    let tab_id = tab.id;
    let needs_render = tab.is_loading && tab.render.rendered_pages.is_empty();
    let is_initial_loading = needs_render;

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
        let dpr = dpr_sig.read().0;
        let db = db.clone();
        spawn(async move {
            if crate::state::commands::open_pdf(&render_tx, &mut tabs, tid, &data_dir, dpr)
                .await
                .is_ok()
            {
                let paper_id = tabs.read().active_tab().and_then(|t| t.paper_id.clone());
                if let Some(ref pid) = paper_id {
                    let mut anns =
                        rotero_db::annotations::list_annotations_for_paper(db.conn(), pid)
                            .await
                            .unwrap_or_default();

                    let pdf_path = tabs.read().tab().pdf_path.clone();
                    // Page pixel dims keyed by absolute page index (rendered_pages
                    // is a sliding window, so it may not be contiguous from 0).
                    let page_dims: std::collections::HashMap<u32, (u32, u32)> = tabs
                        .read()
                        .tab()
                        .render
                        .rendered_pages
                        .values()
                        .map(|p| (p.page_index, (p.width, p.height)))
                        .collect();

                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    if render_tx
                        .send(crate::state::commands::RenderRequest::ExtractAnnotations {
                            pdf_path,
                            reply: reply_tx,
                        })
                        .is_ok()
                        && let Ok(Ok(extracted)) = reply_rx.await
                    {
                        let now = chrono::Utc::now();
                        for ext in extracted {
                            // Deduplicate: skip if a DB annotation exists on same page with same type and similar position
                            let dominated = anns.iter().any(|a| {
                                a.page == ext.page as i32 && a.ann_type == ext.ann_type && {
                                    let ax =
                                        a.geometry.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    let ay =
                                        a.geometry.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                                    let (rw, rh) =
                                        page_dims.get(&ext.page).copied().unwrap_or((1, 1));
                                    let sx = rw as f64 / ext.page_width_pts as f64;
                                    let sy = rh as f64 / ext.page_height_pts as f64;
                                    let ex = ext.rect_pts[0] as f64 * sx;
                                    let ey =
                                        (ext.page_height_pts as f64 - ext.rect_pts[3] as f64) * sy;
                                    (ax - ex).abs() < 10.0 && (ay - ey).abs() < 10.0
                                }
                            });
                            if dominated {
                                continue;
                            }

                            let (rw, rh) = page_dims.get(&ext.page).copied().unwrap_or((1, 1));
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
            // Viewer-local navigation keys (zoom, page/scroll). These operate on
            // this component's scroll container and zoom state, so they stay
            // local rather than going through the global keybinding table. They
            // bubble to the root handler, which ignores them. Global shortcuts
            // like Cmd+F (Find) are owned by `keybindings::BINDINGS`, not here.
            onkeydown: move |evt| {
                match evt.key() {
                    Key::Character(ref c) if c == "+" || c == "=" => {
                        let new_zoom = (zoom + 0.3_f32).min(5.0);
                        crate::state::commands::set_zoom(&mut tabs, tab_id, new_zoom);
                    }
                    Key::Character(ref c) if c == "-" => {
                        let new_zoom = (zoom - 0.3_f32).max(0.5);
                        crate::state::commands::set_zoom(&mut tabs, tab_id, new_zoom);
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

                div {
                    class: "pdf-pages",
                    id: "pdf-pages-container",
                    onscroll: move |_| {
                        if window_loading() {
                            return;
                        }
                        window_loading.set(true);
                        let render_tx = render_ch.sender();
                        let data_dir = config.read().effective_library_path();
                        spawn(async move {
                            // Find the page wrapper whose vertical midpoint is nearest
                            // the container's viewport center; return its page index.
                            // Send the result back via `dioxus.send(...)`; a bare `return`
                            // from the IIFE is NOT delivered to `.recv()` in this Dioxus
                            // version (it comes back as Err(Finished)).
                            let mut eval = document::eval(
                                "let el = document.getElementById('pdf-pages-container'); \
                                 let best = -1; \
                                 if (el) { \
                                   let cr = el.getBoundingClientRect(); \
                                   let mid = cr.top + cr.height / 2; \
                                   let bestDist = Infinity; \
                                   for (let w of el.querySelectorAll('.pdf-page-wrapper')) { \
                                     let r = w.getBoundingClientRect(); \
                                     let c = r.top + r.height / 2; \
                                     let d = Math.abs(c - mid); \
                                     if (d < bestDist) { bestDist = d; \
                                       best = parseInt(w.id.replace('pdf-page-', ''), 10); } \
                                   } \
                                 } \
                                 dioxus.send(best);",
                            );
                            let idx_res = eval.recv::<i64>().await;
                            if let Ok(idx) = idx_res
                                && idx >= 0
                            {
                                crate::state::commands::ensure_window_rendered(
                                    &render_tx,
                                    &mut tabs,
                                    tab_id,
                                    idx as u32,
                                    &data_dir,
                                )
                                .await;
                            }
                            window_loading.set(false);
                        });
                    },
                    onmounted: move |_| {
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
                    {
                        let mgr = tabs.read();
                        let tab = mgr.tab();
                        let pages = tab.render.rendered_pages.clone();
                        let page_dims = tab.render.page_dims.clone();
                        let total = tab.page_count;
                        drop(mgr);
                        // Render every page slot in order with a SINGLE element type
                        // (PdfPageWithOverlay). Resident pages pass Some(image); the rest
                        // pass None and render as a sized placeholder, keeping full scroll
                        // height so every page is reachable (which triggers rendering the
                        // next window). Keeping one node type per key avoids Dioxus
                        // keyed-diff breakage when a slot swaps placeholder<->rendered.
                        rsx! {
                            for idx in 0..total {
                                {
                                    let page = pages.get(&idx);
                                    // Rendered pages carry their exact pixel size; placeholders
                                    // fall back to page_dims (or US-Letter until dims load).
                                    let (w, h) = page
                                        .map(|p| (p.width, p.height))
                                        .or_else(|| page_dims.get(idx as usize).copied())
                                        .unwrap_or((612, 792));
                                    rsx! {
                                        PdfPageWithOverlay {
                                            key: "{idx}",
                                            page_index: idx,
                                            base64_data: page.map(|p| p.base64_data.clone()),
                                            mime: page.map(|p| p.mime).unwrap_or("image/png"),
                                            width: w,
                                            height: h,
                                            zoom,
                                            render_zoom,
                                            tab_id,
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if show_panel {
                    AnnotationPanel { tab_id }
                }
            }

            AnnotationContextMenu {}

        }
    }
}

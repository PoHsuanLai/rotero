use dioxus::prelude::*;

use super::super::chat_panel::ChatToggleButton;
use super::super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use crate::app::RenderChannel;
use crate::state::app_state::{
    DocumentTabManager, LibraryState, LibraryView, PdfTabManager, TabId,
};

#[component]
pub fn PdfTabBar() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();

    // A PDF tab is "active" only when the PDF viewer is the current view; a
    // document tab is active only when its document is the current view.
    let current_view = lib_state.read().view.clone();
    let pdf_is_active_view = matches!(current_view, LibraryView::PdfViewer);

    let mgr = tabs.read();
    let tab_info: Vec<(TabId, String, bool, Option<String>)> = mgr
        .tabs
        .iter()
        .map(|t| {
            (
                t.id,
                t.title.clone(),
                pdf_is_active_view && mgr.active_tab_id == Some(t.id),
                t.paper_id.clone(),
            )
        })
        .collect();
    let tab_count = tab_info.len();
    drop(mgr);

    let mut tab_ctx = use_signal(|| None::<(TabId, Option<String>, usize, f64, f64)>);

    rsx! {
        div { class: "pdf-tab-bar",
            for (idx, (tab_id, title, is_active, paper_id)) in tab_info.iter().enumerate() {
                {
                    let tab_id = *tab_id;
                    let title = title.clone();
                    let is_active = *is_active;
                    let paper_id = paper_id.clone();
                    let tab_class = if is_active { "pdf-tab pdf-tab--active" } else { "pdf-tab" };
                    let display_title = crate::ui::truncate_text(&title, 30);

                    rsx! {
                        div {
                            key: "tab-{tab_id}",
                            class: "{tab_class}",
                            oncontextmenu: {
                                let pid = paper_id.clone();
                                move |evt: Event<MouseData>| {
                                    evt.prevent_default();
                                    tab_ctx.set(Some((tab_id, pid.clone(), idx, evt.client_coordinates().x, evt.client_coordinates().y)));
                                }
                            },
                            onclick: move |_| {
                                if is_active { return; }

                                // Return to the PDF viewer (we may be on a doc tab).
                                lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);

                                let old_tab_id = tabs.read().active_tab_id;
                                let _ = document::eval(
                                    "window.__roteroScrollSave = (function() { \
                                        let el = document.getElementById('pdf-pages-container'); \
                                        return el ? el.scrollTop : 0; \
                                    })()"
                                );

                                tabs.with_mut(|m| m.switch_to(tab_id));

                                if let Some(old_id) = old_tab_id {
                                    spawn(async move {
                                        let mut eval = document::eval("window.__roteroScrollSave || 0");
                                        if let Ok(scroll) = eval.recv::<f64>().await {
                                            tabs.with_mut(|m| {
                                                if let Some(t) = m.tabs.iter_mut().find(|t| t.id == old_id) {
                                                    t.view.scroll_top = scroll;
                                                }
                                            });
                                        }
                                    });
                                }

                                spawn(async move {
                                    let render_tx = render_ch.sender();
                                    let cfg_dir = config.read().effective_library_path();

                                    let needs = tabs.read().active_tab().map(|t| t.needs_render()).unwrap_or(false);
                                    if needs {
                                        tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                        let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, tab_id, &cfg_dir, dpr_sig.read().0).await;
                                    }

                                    // Pages render in a sliding window, so before restoring
                                    // scroll we must render the window around the page the
                                    // user was last on; otherwise that region is blank.
                                    let last_page = tabs.read().active_tab().map(|t| t.view.current_page).unwrap_or(0);
                                    if last_page > 0 {
                                        crate::state::commands::ensure_window_rendered(&render_tx, &mut tabs, tab_id, last_page, &cfg_dir).await;
                                        // Scroll to the page element (robust to variable page heights
                                        // and to Dioxus not having flushed the just-rendered page yet).
                                        let _ = document::eval(&super::scroll_to_page_js(last_page, "start"));
                                    } else {
                                        // Restore exact pixel offset for the top of the document.
                                        let scroll_top = tabs.read().active_tab().map(|t| t.view.scroll_top).unwrap_or(0.0);
                                        let js = format!(
                                            "setTimeout(() => {{ let el = document.getElementById('pdf-pages-container'); if (el) el.scrollTop = {}; }}, 30)",
                                            scroll_top
                                        );
                                        let _ = document::eval(&js);
                                    }
                                });
                            },
                            span { class: "pdf-tab-title", "{display_title}" }
                            button {
                                class: "pdf-tab-close",
                                onclick: move |evt| {
                                    evt.stop_propagation();
                                    // Whether the PDF viewer is what we're currently looking at.
                                    let viewing_pdf = matches!(lib_state.read().view, LibraryView::PdfViewer);
                                    tabs.with_mut(|m| m.close_tab(tab_id));
                                    if tabs.read().tabs.is_empty() {
                                        // Only leave the viewer if we were in it (a doc may be open).
                                        if viewing_pdf {
                                            lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                                        }
                                        // No PDFs open — free the engine's cached file bytes.
                                        let _ = render_ch.sender().send(crate::state::commands::RenderRequest::ClearCache);
                                    } else if viewing_pdf {
                                        let needs = tabs.read().active_tab().map(|t| t.needs_render()).unwrap_or(false);
                                        if needs {
                                            let new_id = tabs.read().active_tab_id.unwrap();
                                            let render_tx = render_ch.sender();
                                            let cfg_dir = config.read().effective_library_path();
                                            tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                            spawn(async move {
                                                let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, new_id, &cfg_dir, dpr_sig.read().0).await;
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

            DocumentTabs {}

            div { style: "flex: 1;" }
            div { style: "padding: 4px 8px 4px 0; display: flex; align-items: center;",
                ChatToggleButton {}
            }

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
                                    let viewing_pdf = matches!(lib_state.read().view, LibraryView::PdfViewer);
                                    tabs.with_mut(|m| m.close_tab(ctx_tab_id));
                                    if tabs.read().tabs.is_empty() {
                                        if viewing_pdf {
                                            lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                                        }
                                        let _ = render_ch.sender().send(crate::state::commands::RenderRequest::ClearCache);
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
                                    on_click: {
                                        let pid = ctx_paper_id.clone();
                                        move |_| {
                                            lib_state.with_mut(|s| {
                                                s.view = LibraryView::AllPapers;
                                                if let Some(pid) = pid.clone() {
                                                    s.select_one(pid);
                                                } else {
                                                    s.clear_selection();
                                                }
                                            });
                                            tab_ctx.set(None);
                                        }
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

/// Renders the open-document tabs inline in the same bar as the PDF tabs. A
/// document tab is highlighted when its document is the current view; clicking
/// switches to it, closing removes it and reroutes the view.
#[component]
fn DocumentTabs() -> Element {
    let mut doc_tabs = use_context::<Signal<DocumentTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();

    let active_doc = match lib_state.read().view.clone() {
        LibraryView::Document(id) => Some(id),
        _ => None,
    };

    let info: Vec<(TabId, String, String, bool)> = doc_tabs
        .read()
        .tabs
        .iter()
        .map(|t| {
            (
                t.id,
                t.doc_id.clone(),
                t.title.clone(),
                active_doc.as_deref() == Some(t.doc_id.as_str()),
            )
        })
        .collect();

    rsx! {
        for (tab_id, doc_id, title, is_active) in info {
            {
                let tab_class = if is_active { "pdf-tab pdf-tab--doc pdf-tab--active" } else { "pdf-tab pdf-tab--doc" };
                let display_title = crate::ui::truncate_text(&title, 30);
                let did_click = doc_id.clone();
                rsx! {
                    div {
                        key: "doctab-{tab_id}",
                        class: "{tab_class}",
                        onclick: move |_| {
                            if is_active { return; }
                            doc_tabs.with_mut(|m| m.switch_to(tab_id));
                            let id = did_click.clone();
                            lib_state.with_mut(|s| s.view = LibraryView::Document(id));
                        },
                        i { class: "bi bi-pencil-square pdf-tab-icon" }
                        span { class: "pdf-tab-title", "{display_title}" }
                        button {
                            class: "pdf-tab-close",
                            onclick: move |evt| {
                                evt.stop_propagation();
                                let was_active = is_active;
                                let now = doc_tabs.with_mut(|m| m.close_tab(tab_id));
                                if was_active {
                                    // Route to the next document tab, or back to the list.
                                    match now {
                                        Some(next_doc) => lib_state.with_mut(|s| s.view = LibraryView::Document(next_doc)),
                                        None => lib_state.with_mut(|s| s.view = LibraryView::Documents),
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

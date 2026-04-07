use dioxus::prelude::*;

use super::super::chat_panel::ChatToggleButton;
use super::super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use crate::app::RenderChannel;
use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager, TabId};
use rotero_pdf::RenderFormat;

#[component]
pub fn PdfTabBar() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

    let mgr = tabs.read();
    let tab_info: Vec<(TabId, String, bool, Option<String>)> = mgr
        .tabs
        .iter()
        .map(|t| {
            (
                t.id,
                t.title.clone(),
                mgr.active_tab_id == Some(t.id),
                t.paper_id.clone(),
            )
        })
        .collect();
    let tab_count = tab_info.len();
    drop(mgr);

    // Tab context menu state: (tab_id, paper_id, tab_index, x, y)
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
                    let display_title = if title.len() > 30 {
                        format!("{}...", &title[..27])
                    } else {
                        title
                    };

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

                                let old_tab_id = tabs.read().active_tab_id;
                                let _ = document::eval(
                                    "window.__roteroScrollSave = (function() { \
                                        let el = document.getElementById('pdf-pages-container'); \
                                        return el ? el.scrollTop : 0; \
                                    })()"
                                );

                                // Switch tab immediately — no awaits
                                tabs.with_mut(|m| m.switch_to(tab_id));

                                // Save old tab's scroll in a separate task (don't block restore)
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

                                // Restore scroll + reload in a separate task
                                spawn(async move {
                                    let scroll_top = tabs.read().active_tab().map(|t| t.view.scroll_top).unwrap_or(0.0);
                                    let js = format!(
                                        "setTimeout(() => {{ let el = document.getElementById('pdf-pages-container'); if (el) el.scrollTop = {}; }}, 30)",
                                        scroll_top
                                    );
                                    let _ = document::eval(&js);

                                    // Reload pages from cache if tab was suspended
                                    let needs = tabs.read().active_tab().map(|t| t.needs_render()).unwrap_or(false);
                                    if needs {
                                        tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                        let render_tx = render_ch.sender();
                                        let cfg = config.read();
                                        let cfg_dir = cfg.effective_library_path();
                                        let cfg_q = cfg.render_quality;
                                        let cfg_fmt = RenderFormat::from_str(&cfg.render_format);
                                        drop(cfg);
                                        let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, tab_id, &cfg_dir, cfg_q, cfg_fmt).await;
                                    }
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
                                            let cfg = config.read();
                                            let cfg_dir = cfg.effective_library_path();
                                            let cfg_q = cfg.render_quality;
                                            let cfg_fmt = RenderFormat::from_str(&cfg.render_format);
                                            drop(cfg);
                                            tabs.with_mut(|m| m.tab_mut().is_loading = true);
                                            spawn(async move {
                                                let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, new_id, &cfg_dir, cfg_q, cfg_fmt).await;
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

            // Spacer + chat toggle
            div { style: "flex: 1;" }
            div { style: "padding: 4px 8px 4px 0; display: flex; align-items: center;",
                ChatToggleButton {}
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
                                    on_click: {
                                        let pid = ctx_paper_id.clone();
                                        move |_| {
                                            lib_state.with_mut(|s| {
                                                s.view = LibraryView::AllPapers;
                                                s.selected_paper_id = pid.clone();
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

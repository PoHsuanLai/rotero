use dioxus::prelude::*;

use crate::state::app_state::{PdfTabManager, TabId};

#[component]
pub(crate) fn PdfSearchBar(tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let docs = use_context::<crate::app::PdfDocs>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
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
                onfocusin: crate::ui::keybindings::editable_focus_in,
                onfocusout: crate::ui::keybindings::editable_focus_out,
                oninput: move |evt| {
                    let new_query = evt.value();
                    let (pdf_path, page_dims) = {
                        let mut guard = tabs.write();
                        let t = guard.tab_mut();
                        t.search.query = new_query.clone();
                        t.search.current_index = 0;
                        if new_query.is_empty() {
                            t.search.matches.clear();
                        }
                        (t.pdf_path.clone(), t.render.page_dims.clone())
                    };
                    if new_query.is_empty() {
                        return;
                    }
                    let docs = docs.get();
                    spawn(async move {
                        match docs.search(pdf_path, new_query.clone(), page_dims).await {
                            Ok(hits) => {
                                tabs.with_mut(|m| {
                                    let t = m.tab_mut();
                                    // Drop stale results if the query changed while searching.
                                    if t.search.query == new_query {
                                        t.search.matches = hits;
                                        t.search.current_index = 0;
                                    }
                                });
                            }
                            Err(e) => tracing::warn!("PDF search failed: {e}"),
                        }
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
                            let docs = docs.get();
                            let data_dir = config.read().effective_library_path();
                            spawn(async move {
                                // Match may be on a page outside the current render window.
                                crate::state::commands::ensure_window_rendered(
                                    &docs, &mut tabs, tab_id, page_idx, &data_dir,
                                ).await;
                                let _ = document::eval(&super::scroll_to_page_js(page_idx, "center"));
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
                onmounted: move |evt| { drop(evt.data().set_focus(true)); },
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

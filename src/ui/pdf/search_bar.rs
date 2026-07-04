use dioxus::prelude::*;

use crate::state::app_state::{PdfTabManager, TabId};

#[component]
pub(crate) fn PdfSearchBar(tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let render_ch = use_context::<crate::app::RenderChannel>();
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
                    // Focus is in this input — claim the key so global shortcuts
                    // (Cmd+F, Cmd+A, Escape, …) don't act on the keystroke the
                    // user is typing here. See keybindings.rs for the precedence
                    // contract.
                    evt.stop_propagation();
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
                            let render_tx = render_ch.sender();
                            let data_dir = config.read().effective_library_path();
                            spawn(async move {
                                // Match may be on a page outside the current render window.
                                crate::state::commands::ensure_window_rendered(
                                    &render_tx, &mut tabs, tab_id, page_idx, &data_dir,
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

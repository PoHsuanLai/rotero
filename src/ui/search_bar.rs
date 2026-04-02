use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::LibraryState;

#[component]
pub fn SearchBar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let query = lib_state.read().search_query.clone();

    rsx! {
        div { class: "search-bar",
            i { class: "search-icon bi bi-search" }
            input {
                class: "search-input",
                r#type: "text",
                placeholder: "Search papers...",
                value: "{query}",
                oninput: move |evt| {
                    let q = evt.value();
                    lib_state.with_mut(|s| s.search_query = q.clone());

                    if q.trim().is_empty() {
                        lib_state.with_mut(|s| s.search_results = None);
                        return;
                    }

                    let db = db.clone();
                    spawn(async move {
                        match crate::db::papers::search_papers(db.conn(), &q).await {
                            Ok(results) => {
                                lib_state.with_mut(|s| s.search_results = Some(results));
                            }
                            Err(_) => {
                                lib_state.with_mut(|s| s.search_results = Some(Vec::new()));
                            }
                        }
                    });
                },
            }
            if !query.is_empty() {
                button {
                    class: "search-clear",
                    onclick: move |_| {
                        lib_state.with_mut(|s| {
                            s.search_query.clear();
                            s.search_results = None;
                        });
                    },
                    i { class: "bi bi-x-lg" }
                }
            }
        }
    }
}

use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, SearchSource};
use rotero_search::parser::metadata_to_paper;

/// Minimum query length before triggering external search.
const MIN_QUERY_LEN: usize = 3;
/// Debounce delay in milliseconds for external API search.
const DEBOUNCE_MS: u64 = 250;

#[component]
pub fn SearchBar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let query = lib_state.read().search_query.clone();
    let source = lib_state.read().search_source.clone();
    let mut show_dropdown = use_signal(|| false);

    // Generation counter for debouncing — incremented on each keystroke,
    // the async task only applies results if its generation is still current.
    let mut search_gen = use_signal(|| 0u64);

    let is_external = source != SearchSource::Local;

    let placeholder = match source {
        SearchSource::Local => "Search papers...",
        SearchSource::OpenAlex => "Search OpenAlex...",
        SearchSource::ArXiv => "Search arXiv...",
        SearchSource::SemanticScholar => "Search Semantic Scholar...",
    };

    rsx! {
        div { class: "search-bar",
            i { class: "search-icon bi bi-search" }
            input {
                id: "library-search-input",
                class: "input input--lg search-input",
                r#type: "text",
                placeholder: "{placeholder}",
                value: "{query}",
                oninput: move |evt| {
                    let q = evt.value();
                    lib_state.with_mut(|s| s.search_query = q.clone());

                    let current_source = lib_state.read().search_source.clone();

                    if current_source == SearchSource::Local {
                        // Local: instant live search (no debounce needed)
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
                    } else {
                        // External: debounced live search
                        if q.trim().len() < MIN_QUERY_LEN {
                            lib_state.with_mut(|s| {
                                s.external_results = None;
                                s.external_searching = false;
                            });
                            return;
                        }

                        // Bump generation and mark as searching
                        let generation = search_gen() + 1;
                        search_gen.set(generation);
                        lib_state.with_mut(|s| s.external_searching = true);

                        let source = current_source.clone();
                        spawn(async move {
                            // Wait for debounce period
                            tokio::time::sleep(std::time::Duration::from_millis(DEBOUNCE_MS)).await;

                            // Check if this is still the latest keystroke
                            if search_gen() != generation {
                                return;
                            }

                            let q = lib_state.read().search_query.clone();
                            if q.trim().len() < MIN_QUERY_LEN {
                                return;
                            }

                            let results = run_external_search(&source, &q).await;

                            // Only apply if still the latest generation
                            if search_gen() == generation {
                                lib_state.with_mut(|s| {
                                    s.external_searching = false;
                                    s.external_results = Some(results);
                                });
                            }
                        });
                    }
                },
            }

            if !query.is_empty() && !is_external {
                button {
                    class: "btn btn--ghost btn--sm search-save",
                    title: "Save this search",
                    onclick: {
                        let query_to_save = query.clone();
                        let db_save = db.clone();
                        move |_| {
                            let q = query_to_save.clone();
                            let db = db_save.clone();
                            spawn(async move {
                                let search = rotero_models::SavedSearch::new(q.clone(), q);
                                let _ = crate::db::saved_searches::insert_saved_search(db.conn(), &search).await;
                                if let Ok(searches) = crate::db::saved_searches::list_saved_searches(db.conn()).await {
                                    lib_state.with_mut(|s| s.saved_searches = searches);
                                }
                            });
                        }
                    },
                    i { class: "bi bi-bookmark-plus" }
                }
            }
            if !query.is_empty() {
                button {
                    class: "search-clear",
                    onclick: move |_| {
                        search_gen.set(search_gen() + 1);
                        lib_state.with_mut(|s| {
                            s.search_query.clear();
                            s.search_results = None;
                            s.external_results = None;
                            s.external_searching = false;
                        });
                    },
                    i { class: "bi bi-x-lg" }
                }
            }

            // Source dropdown
            div { class: "search-source-wrapper",
                button {
                    class: if is_external { "btn btn--sm search-source-btn search-source-btn--active" } else { "btn btn--sm search-source-btn" },
                    onclick: move |_| show_dropdown.toggle(),
                    "{source.label()}"
                    i { class: "bi bi-chevron-down search-source-chevron" }
                }
                if show_dropdown() {
                    div { class: "search-source-dropdown",
                        for src in SearchSource::all().iter() {
                            {
                                let s = src.clone();
                                let label = src.label();
                                let is_active = *src == source;
                                rsx! {
                                    button {
                                        key: "{label}",
                                        class: if is_active { "search-source-option search-source-option--active" } else { "search-source-option" },
                                        onclick: move |_| {
                                            search_gen.set(search_gen() + 1);
                                            lib_state.with_mut(|st| {
                                                st.search_source = s.clone();
                                                st.search_results = None;
                                                st.external_results = None;
                                                st.external_searching = false;
                                            });
                                            show_dropdown.set(false);
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    // Invisible backdrop to close dropdown
                    div {
                        class: "search-source-backdrop",
                        onclick: move |_| show_dropdown.set(false),
                    }
                }
            }
        }
    }
}

async fn run_external_search(source: &SearchSource, query: &str) -> Vec<rotero_models::Paper> {
    let provider = match source.provider() {
        Some(p) => p,
        None => return Vec::new(),
    };

    match provider.search(query, 20).await {
        Ok(metas) => metas.into_iter().map(metadata_to_paper).collect(),
        Err(e) => {
            eprintln!("External search error: {e}");
            Vec::new()
        }
    }
}

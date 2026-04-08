use dioxus::prelude::*;
use futures_util::StreamExt;

use crate::state::app_state::{LibraryState, SearchSource};
use rotero_db::Database;

const MIN_QUERY_LEN: usize = 3;
const DEBOUNCE_MS: u64 = 250;

#[derive(Clone)]
enum SearchMsg {
    Query(SearchSource, String),
    Clear,
}

#[component]
pub fn SearchBar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let (query, source) = {
        let state = lib_state.read();
        (state.search.query.clone(), state.search.source.clone())
    };
    let mut show_dropdown = use_signal(|| false);

    let search_coro = use_coroutine(move |mut rx: UnboundedReceiver<SearchMsg>| async move {
        while let Some(msg) = rx.next().await {
            match msg {
                SearchMsg::Clear => {
                    lib_state.with_mut(|s| {
                        s.search.external_results = None;
                        s.search.external_searching = false;
                    });
                }
                SearchMsg::Query(source, query) => {
                    let (source, query) = drain_latest(&mut rx, source, query);

                    if query.trim().len() < MIN_QUERY_LEN {
                        lib_state.with_mut(|s| {
                            s.search.external_results = None;
                            s.search.external_searching = false;
                        });
                        continue;
                    }

                    lib_state.with_mut(|s| s.search.external_searching = true);

                    tokio::time::sleep(std::time::Duration::from_millis(DEBOUNCE_MS)).await;
                    let (source, query) = drain_latest(&mut rx, source, query);

                    if query.trim().len() < MIN_QUERY_LEN {
                        lib_state.with_mut(|s| {
                            s.search.external_results = None;
                            s.search.external_searching = false;
                        });
                        continue;
                    }

                    let Some(provider) = source.provider() else {
                        continue;
                    };

                    match provider.search(&query, 20).await {
                        Ok(metas) => {
                            let papers: Vec<_> = metas;
                            lib_state.with_mut(|s| {
                                s.search.external_searching = false;
                                s.search.external_results = Some(papers);
                            });
                        }
                        Err(e) => {
                            tracing::error!("External search error: {e}");
                            lib_state.with_mut(|s| {
                                s.search.external_searching = false;
                                s.search.external_results = Some(Vec::new());
                            });
                        }
                    }

                    if provider.needs_enrichment() {
                        if rx.try_recv().is_ok() {
                            continue;
                        }
                        match provider.search_full(&query, 20).await {
                            Ok(metas) => {
                                let papers: Vec<_> = metas;
                                if lib_state.read().search.query == query {
                                    lib_state
                                        .with_mut(|s| s.search.external_results = Some(papers));
                                }
                            }
                            Err(e) => {
                                tracing::error!("Full search enrichment error: {e}");
                            }
                        }
                    }
                }
            }
        }
    });

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
                    lib_state.with_mut(|s| s.search.query = q.clone());

                    let current_source = lib_state.read().search.source.clone();

                    if current_source == SearchSource::Local {
                        if q.trim().is_empty() {
                            lib_state.with_mut(|s| s.search.results = None);
                            return;
                        }
                        let db = db.clone();
                        spawn(async move {
                            match rotero_db::papers::search_papers(db.conn(), &q).await {
                                Ok(results) => {
                                    lib_state.with_mut(|s| s.search.results = Some(results));
                                }
                                Err(_) => {
                                    lib_state.with_mut(|s| s.search.results = Some(Vec::new()));
                                }
                            }
                        });
                    } else {
                        search_coro.send(SearchMsg::Query(current_source, q));
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
                                let _ = rotero_db::saved_searches::insert_saved_search(db.conn(), &search).await;
                                if let Ok(searches) = rotero_db::saved_searches::list_saved_searches(db.conn()).await {
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
                        search_coro.send(SearchMsg::Clear);
                        lib_state.with_mut(|s| {
                            s.search.query.clear();
                            s.search.results = None;
                            s.search.external_results = None;
                            s.search.external_searching = false;
                        });
                    },
                    i { class: "bi bi-x-lg" }
                }
            }

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
                                            search_coro.send(SearchMsg::Clear);
                                            lib_state.with_mut(|st| {
                                                st.search.source = s.clone();
                                                st.search.results = None;
                                                st.search.external_results = None;
                                                st.search.external_searching = false;
                                            });
                                            show_dropdown.set(false);
                                        },
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "search-source-backdrop",
                        onclick: move |_| show_dropdown.set(false),
                    }
                }
            }
        }
    }
}

fn drain_latest(
    rx: &mut UnboundedReceiver<SearchMsg>,
    source: SearchSource,
    query: String,
) -> (SearchSource, String) {
    let mut latest_source = source;
    let mut latest_query = query;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            SearchMsg::Query(s, q) => {
                latest_source = s;
                latest_query = q;
            }
            SearchMsg::Clear => {
                latest_query = String::new();
            }
        }
    }
    (latest_source, latest_query)
}

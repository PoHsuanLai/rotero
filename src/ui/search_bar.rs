use dioxus::prelude::*;
use futures_util::StreamExt;

use crate::state::app_state::{LibraryState, ProviderResults};
use rotero_db::Database;
use rotero_search::SearchProvider;

const MIN_QUERY_LEN: usize = 3;
const DEBOUNCE_MS: u64 = 250;

#[derive(Clone)]
enum SearchMsg {
    Query(String),
    Clear,
}

#[component]
pub fn SearchBar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let query = lib_state.read().search.query.clone();

    let search_coro = use_coroutine({
        let db = db.clone();
        move |mut rx: UnboundedReceiver<SearchMsg>| {
            let db = db.clone();
            async move {
            while let Some(msg) = rx.next().await {
                let query = match msg {
                    SearchMsg::Clear => {
                        lib_state.with_mut(|s| s.search.reset_results());
                        continue;
                    }
                    SearchMsg::Query(q) => drain_latest(&mut rx, q),
                };

                if query.trim().len() < MIN_QUERY_LEN {
                    lib_state.with_mut(|s| s.search.reset_results());
                    continue;
                }

                // Debounce, then coalesce anything typed during the sleep.
                tokio::time::sleep(std::time::Duration::from_millis(DEBOUNCE_MS)).await;
                let query = drain_latest(&mut rx, query);
                if query.trim().len() < MIN_QUERY_LEN {
                    lib_state.with_mut(|s| s.search.reset_results());
                    continue;
                }

                // Commit this query: bump the generation, mark every provider
                // in-flight and clear stale results so old hits don't linger
                // under the new query.
                let generation = lib_state.with_mut(|s| {
                    s.search.generation += 1;
                    s.search.results = None;
                    s.search.openalex = ProviderResults { results: None, searching: true };
                    s.search.arxiv = ProviderResults { results: None, searching: true };
                    s.search.semantic_scholar =
                        ProviderResults { results: None, searching: true };
                    s.search.generation
                });

                // Fan out. Each task writes only its own slice, guarded by the
                // generation, and nothing awaits anything else — local and each
                // provider render the instant they return.
                {
                    let db = db.clone();
                    let q = query.clone();
                    spawn(async move {
                        let results = db.search_papers(&q).await.unwrap_or_default();
                        lib_state.with_mut(|s| {
                            if s.search.generation == generation {
                                s.search.results = Some(results);
                            }
                        });
                    });
                }

                spawn_provider(SearchProvider::OpenAlex, generation, query.clone(), lib_state);
                spawn_provider(SearchProvider::ArXiv, generation, query.clone(), lib_state);
                spawn_provider(
                    SearchProvider::SemanticScholar,
                    generation,
                    query.clone(),
                    lib_state,
                );
            }
            }
        }
    });

    rsx! {
        div { class: "search-bar",
            i { class: "search-icon bi bi-search" }
            input {
                id: "library-search-input",
                class: "input input--lg search-input",
                r#type: "text",
                placeholder: "Search your library and the web...",
                value: "{query}",
                oninput: move |evt| {
                    let q = evt.value();
                    lib_state.with_mut(|s| s.search.query = q.clone());
                    if q.trim().is_empty() {
                        search_coro.send(SearchMsg::Clear);
                    } else {
                        search_coro.send(SearchMsg::Query(q));
                    }
                },
            }

            if !query.is_empty() {
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
                                let _ = db.insert_saved_search(&search).await;
                                if let Ok(searches) = db.list_saved_searches().await {
                                    lib_state.with_mut(|s| s.saved_searches = searches);
                                }
                            });
                        }
                    },
                    i { class: "bi bi-bookmark-plus" }
                }
                button {
                    class: "search-clear",
                    onclick: move |_| {
                        search_coro.send(SearchMsg::Clear);
                        lib_state.with_mut(|s| {
                            s.search.query.clear();
                            s.search.reset_results();
                        });
                    },
                    i { class: "bi bi-x-lg" }
                }
            }
        }
    }
}

/// Spawns one provider search task. Writes only that provider's slice, guarded
/// by `generation`, and runs OpenAlex's two-phase enrichment within the same
/// task so it stays independent of the other providers.
fn spawn_provider(
    provider: SearchProvider,
    generation: u64,
    query: String,
    mut lib_state: Signal<LibraryState>,
) {
    spawn(async move {
        match provider.search(&query, 20).await {
            Ok(papers) => lib_state.with_mut(|s| {
                if s.search.generation == generation {
                    let slot = provider_slot(provider, s);
                    slot.results = Some(papers);
                    // Stay "searching" if a richer follow-up call is coming.
                    slot.searching = provider.needs_enrichment();
                }
            }),
            Err(e) => {
                tracing::error!("{} search error: {e}", provider.name());
                lib_state.with_mut(|s| {
                    if s.search.generation == generation {
                        let slot = provider_slot(provider, s);
                        slot.results = Some(Vec::new());
                        slot.searching = false;
                    }
                });
            }
        }

        if provider.needs_enrichment() {
            match provider.search_full(&query, 20).await {
                Ok(papers) => lib_state.with_mut(|s| {
                    if s.search.generation == generation {
                        let slot = provider_slot(provider, s);
                        slot.results = Some(papers);
                        slot.searching = false;
                    }
                }),
                Err(e) => {
                    tracing::error!("{} enrichment error: {e}", provider.name());
                    lib_state.with_mut(|s| {
                        if s.search.generation == generation {
                            provider_slot(provider, s).searching = false;
                        }
                    });
                }
            }
        }
    });
}

/// Returns the mutable state slice belonging to a provider.
fn provider_slot(provider: SearchProvider, state: &mut LibraryState) -> &mut ProviderResults {
    match provider {
        SearchProvider::OpenAlex => &mut state.search.openalex,
        SearchProvider::ArXiv => &mut state.search.arxiv,
        SearchProvider::SemanticScholar => &mut state.search.semantic_scholar,
    }
}

fn drain_latest(rx: &mut UnboundedReceiver<SearchMsg>, query: String) -> String {
    let mut latest_query = query;
    while let Ok(msg) = rx.try_recv() {
        match msg {
            SearchMsg::Query(q) => latest_query = q,
            SearchMsg::Clear => latest_query = String::new(),
        }
    }
    latest_query
}

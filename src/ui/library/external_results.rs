use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use crate::state::commands::ImportChannel;

/// The "From the web" panel: a single consolidated list of results merged and
/// deduplicated across all online providers, ranked by relevance. The merge
/// recomputes as each provider streams in, so the list grows and re-ranks
/// without waiting for the slowest source.
///
/// Clicking a row opens its overview in the detail panel; importing routes
/// through the app-root [`ImportChannel`] so the insert + OA-PDF download
/// survives the search being cleared.
#[component]
pub(crate) fn ExternalResults() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let import_channel = use_context::<ImportChannel>();

    // Merged, deduped, ranked results. Recomputes whenever any provider slot,
    // the query, or the library changes.
    let merged = use_memo(move || {
        let state = lib_state.read();
        let batches: Vec<Vec<rotero_models::Paper>> = vec![
            state.search.openalex.results.clone().unwrap_or_default(),
            state.search.arxiv.results.clone().unwrap_or_default(),
            state.search.semantic_scholar.results.clone().unwrap_or_default(),
        ];
        rotero_models::merge_and_rank(&batches, &state.search.query)
    });

    let (existing_dois, still_searching, previewed_key) = {
        let state = lib_state.read();
        let dois: std::collections::HashSet<String> = state
            .papers
            .iter()
            .filter_map(|p| p.doi.clone())
            .filter(|d| !d.is_empty())
            .collect();
        let previewed = state.previewed_web.as_ref().map(result_key);
        (dois, state.search.web_searching(), previewed)
    };

    let results = merged.read().clone();

    if results.is_empty() {
        // Nothing yet — say so only once every provider has finished, so the
        // header spinner (driven by web_searching) isn't contradicted.
        if still_searching {
            return rsx! { div { class: "library-list" } };
        }
        return rsx! {
            div { class: "library-list",
                div { class: "external-provider-empty", "No results from the web." }
            }
        };
    }

    let importable_count = results
        .iter()
        .filter(|p| !is_imported(p, &existing_dois))
        .count();
    let all_imported = importable_count == 0;

    rsx! {
        div { class: "library-list",
            div { class: "external-results-banner",
                span { "{results.len()} results from the web" }
                button {
                    class: "btn btn--sm btn--primary",
                    disabled: all_imported,
                    onclick: move |_| {
                        let existing = existing_dois.clone();
                        for paper in merged.read().iter() {
                            if !is_imported(paper, &existing) {
                                import_channel.import(paper.clone());
                            }
                        }
                    },
                    if all_imported { "All Imported" } else { "Import All" }
                }
            }
            for paper in results.iter() {
                {
                    let title = paper.title.clone();
                    let authors = paper.formatted_authors();
                    let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                    let journal = paper.publication.journal.clone().unwrap_or_default();
                    let citation_count = paper.citation.citation_count;
                    let doi = paper.doi.clone().unwrap_or_default();
                    let abstract_text = paper.abstract_text.clone().unwrap_or_default();
                    let has_abstract = !abstract_text.is_empty();
                    let already_imported = is_imported(paper, &existing_dois);
                    let row_key = result_key(paper);
                    let selected = previewed_key.as_deref() == Some(row_key.as_str());
                    let paper_select = paper.clone();
                    let paper_import = paper.clone();

                    let card_class = if selected {
                        "library-card external-result-card external-result-card--selected"
                    } else if already_imported {
                        "library-card external-result-card external-result-card--imported"
                    } else {
                        "library-card external-result-card"
                    };

                    rsx! {
                        div {
                            key: "{row_key}",
                            class: "{card_class}",
                            onclick: move |_| {
                                lib_state.with_mut(|s| s.preview_web(paper_select.clone()));
                            },

                            div { class: "library-card-body",
                                div { class: "library-card-title", "{title}" }
                                div { class: "library-card-meta",
                                    span { class: "library-card-authors", "{authors}" }
                                    if !year.is_empty() {
                                        span { class: "library-card-sep", "\u{00b7}" }
                                        span { class: "library-card-year", "{year}" }
                                    }
                                    if !journal.is_empty() {
                                        span { class: "library-card-sep", "\u{00b7}" }
                                        span { class: "library-card-journal", "{journal}" }
                                    }
                                    if let Some(count) = citation_count {
                                        span { class: "library-card-sep", "\u{00b7}" }
                                        span { class: "library-card-citations", title: "Citation count", "{count} cited" }
                                    }
                                    if !doi.is_empty() {
                                        span { class: "library-card-sep", "\u{00b7}" }
                                        span { class: "library-card-doi", "{doi}" }
                                    }
                                }
                                if has_abstract {
                                    div { class: "external-result-abstract", "{abstract_text}" }
                                }
                            }

                            div { class: "library-card-actions",
                                if already_imported {
                                    button {
                                        class: "btn btn--sm btn--ghost external-imported-btn",
                                        disabled: true,
                                        i { class: "bi bi-check-lg" }
                                        "Imported"
                                    }
                                } else {
                                    button {
                                        class: "btn btn--sm btn--primary",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            import_channel.import(paper_import.clone());
                                        },
                                        "Import"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A web result counts as imported when a library paper carries the same
/// non-empty DOI.
fn is_imported(paper: &rotero_models::Paper, existing_dois: &std::collections::HashSet<String>) -> bool {
    paper
        .doi
        .as_ref()
        .is_some_and(|d| !d.is_empty() && existing_dois.contains(d))
}

/// Stable identity for a merged result row, so Dioxus diffing survives the list
/// re-ranking as slower providers stream in. Also used to mark the previewed
/// row as selected.
fn result_key(paper: &rotero_models::Paper) -> String {
    match paper.paper_id() {
        Some(pid) => pid.to_stored_string(),
        None => rotero_models::normalize_title(&paper.title),
    }
}

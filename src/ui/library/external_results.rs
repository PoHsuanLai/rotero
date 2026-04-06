use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

#[component]
pub(crate) fn ExternalResults(results: Vec<rotero_models::Paper>, searching: bool) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let source_label = lib_state.read().search.source.label();

    // Collect DOIs already in the library for duplicate detection
    let existing_dois: std::collections::HashSet<String> = lib_state
        .read()
        .papers
        .iter()
        .filter_map(|p| p.doi.clone())
        .filter(|d| !d.is_empty())
        .collect();

    rsx! {
        div { class: "library-list",
            if searching {
                div { class: "external-search-status",
                    i { class: "bi bi-arrow-repeat external-spinner" }
                    "Searching {source_label}..."
                }
            } else if results.is_empty() {
                div { class: "library-empty",
                    if lib_state.read().search.query.is_empty() {
                        p { class: "library-empty-heading", "Search {source_label}" }
                        p { class: "library-empty-sub", "Type a query and press Enter to search." }
                    } else if lib_state.read().search.external_results.is_some() {
                        p { class: "library-empty-heading", "No results found" }
                        p { class: "library-empty-sub", "Try a different search term." }
                    } else {
                        p { class: "library-empty-heading", "Search {source_label}" }
                        p { class: "library-empty-sub", "Press Enter to search." }
                    }
                }
            } else {
                {
                    let importable_count = results.iter().filter(|p| {
                        p.doi.as_ref().is_none_or(|d| d.is_empty() || !existing_dois.contains(d))
                    }).count();
                    let all_imported = importable_count == 0;
                    let db_banner = db.clone();

                    rsx! {
                        // Banner
                        div { class: "external-results-banner",
                            span { "{results.len()} results from {source_label}" }
                            button {
                                class: "btn btn--sm btn--primary",
                                disabled: all_imported,
                                onclick: move |_| {
                                    let state = lib_state.read();
                                    let papers = state.search.external_results.clone().unwrap_or_default();
                                    let existing: std::collections::HashSet<String> = state.papers.iter()
                                        .filter_map(|p| p.doi.clone())
                                        .filter(|d| !d.is_empty())
                                        .collect();
                                    drop(state);
                                    let db = db_banner.clone();
                                    spawn(async move {
                                        let mut imported = 0;
                                        for paper in papers {
                                            // Skip papers with DOIs we already have
                                            if let Some(ref doi) = paper.doi {
                                                if !doi.is_empty() && existing.contains(doi) {
                                                    continue;
                                                }
                                            }
                                            if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                                                let mut paper = paper;
                                                paper.id = Some(id);
                                                lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                imported += 1;
                                            }
                                        }
                                        eprintln!("Imported {imported} papers");
                                    });
                                },
                                if all_imported { "All Imported" } else { "Import All" }
                            }
                        }
                    }
                }
                for (i, paper) in results.iter().enumerate() {
                    {
                        let title = paper.title.clone();
                        let authors = if paper.authors.is_empty() {
                            "Unknown".to_string()
                        } else if paper.authors.len() <= 2 {
                            paper.authors.join(", ")
                        } else {
                            format!("{} et al.", paper.authors[0])
                        };
                        let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                        let journal = paper.journal.clone().unwrap_or_default();
                        let citation_count = paper.citation_count;
                        let doi = paper.doi.clone().unwrap_or_default();
                        let abstract_text = paper.abstract_text.clone().unwrap_or_default();
                        let has_abstract = !abstract_text.is_empty();
                        let already_imported = !doi.is_empty() && existing_dois.contains(&doi);
                        let paper_clone = paper.clone();
                        let db_import = db.clone();

                        rsx! {
                            div {
                                key: "ext-{i}",
                                class: if already_imported { "library-card external-result-card external-result-card--imported" } else { "library-card external-result-card" },

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
                                            onclick: move |_| {
                                                let paper = paper_clone.clone();
                                                let db = db_import.clone();
                                                spawn(async move {
                                                    // If we have a DOI but sparse metadata (autocomplete result),
                                                    // fetch full details first
                                                    let paper = enrich_before_import(paper).await;
                                                    match rotero_db::papers::insert_paper(db.conn(), &paper).await {
                                                        Ok(id) => {
                                                            let mut paper = paper;
                                                            paper.id = Some(id);
                                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                        }
                                                        Err(e) => eprintln!("Failed to import: {e}"),
                                                    }
                                                });
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
}

/// If a paper has a DOI but missing authors (e.g. from autocomplete),
/// try to fetch full metadata before inserting.
async fn enrich_before_import(paper: rotero_models::Paper) -> rotero_models::Paper {
    // Only enrich if we have a DOI but are missing basic fields
    let needs_enrichment =
        paper.authors.is_empty() && paper.doi.as_ref().is_some_and(|d| !d.is_empty());
    if !needs_enrichment {
        return paper;
    }

    let doi = paper.doi.as_deref().unwrap_or_default();
    // Try OpenAlex full endpoint first (fastest), then CrossRef
    if let Ok(meta) = crate::metadata::openalex::fetch_by_doi(doi).await {
        return crate::metadata::parser::metadata_to_paper(meta);
    }
    if let Ok(meta) = crate::metadata::crossref::fetch_by_doi(doi).await {
        return crate::metadata::parser::metadata_to_paper(meta);
    }
    paper
}

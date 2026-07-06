use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

/// The "From the web" panel: a single consolidated list of results merged and
/// deduplicated across all online providers, ranked by relevance. The merge
/// recomputes as each provider streams in, so the list grows and re-ranks
/// without waiting for the slowest source.
#[component]
pub(crate) fn ExternalResults() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

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

    let (existing_dois, still_searching) = {
        let state = lib_state.read();
        let dois: std::collections::HashSet<String> = state
            .papers
            .iter()
            .filter_map(|p| p.doi.clone())
            .filter(|d| !d.is_empty())
            .collect();
        (dois, state.search.web_searching())
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
        .filter(|p| {
            p.doi
                .as_ref()
                .is_none_or(|d| d.is_empty() || !existing_dois.contains(d))
        })
        .count();
    let all_imported = importable_count == 0;
    let db_banner = db.clone();

    rsx! {
        div { class: "library-list",
            div { class: "external-results-banner",
                span { "{results.len()} results from the web" }
                button {
                    class: "btn btn--sm btn--primary",
                    disabled: all_imported,
                    onclick: move |_| {
                        let papers = merged.read().clone();
                        let existing: std::collections::HashSet<String> = lib_state.read().papers.iter()
                            .filter_map(|p| p.doi.clone())
                            .filter(|d| !d.is_empty())
                            .collect();
                        let db = db_banner.clone();
                        spawn(async move {
                            let mut imported_papers: Vec<(rotero_models::Paper, String)> = Vec::new();
                            for paper in papers {
                                if let Some(ref doi) = paper.doi
                                    && !doi.is_empty() && existing.contains(doi) {
                                        continue;
                                    }
                                let paper = enrich_before_import(paper).await;
                                if let Ok(id) = db.insert_paper(&paper).await {
                                    let mut paper = paper;
                                    paper.id = Some(id.clone());
                                    lib_state.with_mut(|s| s.papers.insert(0, paper.clone()));
                                    imported_papers.push((paper, id));
                                }
                            }
                            tracing::info!("Imported {} papers, starting OA PDF downloads", imported_papers.len());
                            for (paper, paper_id) in imported_papers {
                                try_download_oa_pdf(&db, &mut lib_state, &paper, &paper_id).await;
                            }
                        });
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
                    let already_imported = !doi.is_empty() && existing_dois.contains(&doi);
                    let row_key = result_key(paper);
                    let paper_clone = paper.clone();
                    let db_import = db.clone();

                    rsx! {
                        div {
                            key: "{row_key}",
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
                                                match db.insert_paper(&paper).await {
                                                    Ok(id) => {
                                                        let mut paper = paper;
                                                        paper.id = Some(id.clone());
                                                        lib_state.with_mut(|s| s.papers.insert(0, paper.clone()));
                                                        // Auto-download OA PDF
                                                        try_download_oa_pdf(&db, &mut lib_state, &paper, &id).await;
                                                    }
                                                    Err(e) => tracing::error!("Failed to import: {e}"),
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

/// Stable identity for a merged result row, so Dioxus diffing survives the list
/// re-ranking as slower providers stream in.
fn result_key(paper: &rotero_models::Paper) -> String {
    match paper.paper_id() {
        Some(pid) => pid.to_stored_string(),
        None => rotero_models::normalize_title(&paper.title),
    }
}

async fn try_download_oa_pdf(
    db: &Database,
    lib_state: &mut Signal<LibraryState>,
    paper: &rotero_models::Paper,
    paper_id: &str,
) {
    let title = &paper.title;
    tracing::info!("Auto-downloading OA PDF for: {title}");
    match crate::metadata::pdf_download::find_and_download_pdf(
        db,
        paper.doi.as_deref(),
        &paper.title,
        paper.authors.first().map(|a| a.as_str()),
        paper.year,
    )
    .await
    {
        Ok(rel_path) => {
            let _ = db.update_pdf_path(paper_id, &rel_path).await;
            let pid = paper_id.to_string();
            lib_state.with_mut(|s| {
                if let Some(p) = s
                    .papers
                    .iter_mut()
                    .find(|p| p.id.as_deref() == Some(pid.as_str()))
                {
                    p.links.pdf_path = Some(rel_path.clone());
                }
            });
            tracing::info!("Auto-downloaded OA PDF for: {title} -> {rel_path}");
        }
        Err(e) => {
            tracing::debug!("No OA PDF for: {title}: {e}");
        }
    }
}

/// If a paper has a DOI but missing authors, fetch full metadata before inserting.
async fn enrich_before_import(paper: rotero_models::Paper) -> rotero_models::Paper {
    let needs_enrichment =
        paper.authors.is_empty() && paper.doi.as_ref().is_some_and(|d| !d.is_empty());
    if !needs_enrichment {
        return paper;
    }

    let doi = paper.doi.as_deref().unwrap_or_default();
    if let Ok(enriched) = crate::metadata::openalex::fetch_by_doi(doi).await {
        return enriched;
    }
    if let Ok(enriched) = crate::metadata::crossref::fetch_by_doi(doi).await {
        return enriched;
    }
    paper
}

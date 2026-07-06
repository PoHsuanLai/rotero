use dioxus::prelude::*;
use futures_util::StreamExt;
use rotero_db::Database;
use rotero_models::Paper;

use crate::state::app_state::LibraryState;

/// A paper to bring into the library, along with what should happen after it
/// lands: metadata enrichment and an open-access PDF download.
pub struct ImportRequest {
    pub paper: Paper,
}

/// Insert a paper into the library and, in the background, download its
/// open-access PDF.
///
/// This runs in the app-root coroutine (see [`use_import_coroutine`]) rather
/// than a component event handler so the download future outlives the card or
/// panel that triggered it — clearing the search or navigating away no longer
/// cancels an in-flight download.
///
/// Sparse web results (a DOI but no authors, typical of autocomplete hits) are
/// enriched from OpenAlex/CrossRef before insertion so the stored record and
/// the PDF filename are complete.
async fn run_import(db: &Database, mut lib_state: Signal<LibraryState>, paper: Paper) {
    let paper = enrich_before_import(paper).await;

    let id = match db.insert_paper(&paper).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Import failed: {e}");
            return;
        }
    };

    let mut paper = paper;
    paper.id = Some(id.clone());
    lib_state.with_mut(|s| s.papers.insert(0, paper.clone()));

    download_pdf_into_library(db, &mut lib_state, &paper, &id).await;
}

/// Resolve and download the open-access PDF for an already-imported paper,
/// writing the resulting path back into both the DB and the library signal.
/// Failure is non-fatal — many papers simply have no OA copy.
pub async fn download_pdf_into_library(
    db: &Database,
    lib_state: &mut Signal<LibraryState>,
    paper: &Paper,
    paper_id: &str,
) {
    let title = paper.title.clone();
    let author_names = paper.author_names();
    tracing::info!("Downloading OA PDF for: {title}");
    match crate::metadata::pdf_download::find_and_download_pdf(
        db,
        paper.links.pdf_url.as_deref(),
        paper.doi.as_deref(),
        &paper.title,
        author_names.first().map(|a| a.as_str()),
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
            tracing::info!("Downloaded OA PDF for: {title} -> {rel_path}");
        }
        Err(e) => {
            tracing::debug!("No OA PDF for: {title}: {e}");
        }
    }
}

/// If a paper has a DOI but missing authors, fetch full metadata before
/// inserting so the stored record isn't a sparse autocomplete stub.
async fn enrich_before_import(paper: Paper) -> Paper {
    let needs_enrichment =
        paper.author_names().is_empty() && paper.doi.as_ref().is_some_and(|d| !d.is_empty());
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

/// Handle to the app-root import coroutine. Cloneable and `Copy`, so any
/// component can queue an import without threading the DB/signal through.
#[derive(Clone, Copy)]
pub struct ImportChannel {
    inner: Coroutine<ImportRequest>,
}

impl ImportChannel {
    /// Queue a paper for import + OA download. Returns immediately; the work
    /// runs in the app-root scope and survives the caller unmounting.
    pub fn import(&self, paper: Paper) {
        self.inner.send(ImportRequest { paper });
    }
}

/// Spawn the app-root import coroutine and expose it via context. Call once,
/// near the top of the component tree, after the `Database` and `LibraryState`
/// signal are in context.
pub fn use_import_coroutine(db: Database, lib_state: Signal<LibraryState>) -> ImportChannel {
    let coro = use_coroutine(move |mut rx: UnboundedReceiver<ImportRequest>| {
        let db = db.clone();
        async move {
            while let Some(req) = rx.next().await {
                run_import(&db, lib_state, req.paper).await;
            }
        }
    });
    ImportChannel { inner: coro }
}

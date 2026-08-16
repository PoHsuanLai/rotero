use dioxus::prelude::*;
use futures_util::StreamExt;
use rotero_db::Database;
use rotero_models::Paper;

use crate::state::app_state::{ImportStatus, LibraryState};

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
    let key = import_key(&paper);
    set_status(&mut lib_state, &key, ImportStatus::Importing);

    let paper = enrich_before_import(paper).await;

    let id = match db.insert_paper(&paper).await {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Import failed: {e}");
            // Surfaced rather than only logged: an insert that commits its row
            // and then fails change tracking is invisible otherwise, which is
            // how a broken library looked like a no-op button.
            set_status(&mut lib_state, &key, ImportStatus::Failed(e.to_string()));
            return;
        }
    };

    let mut paper = paper;
    paper.id = Some(id.clone());
    lib_state.with_mut(|s| {
        s.papers.insert(0, paper.clone());
        // The paper is in the library now, so the list's own DOI check takes
        // over as the source of truth for "imported".
        s.import_status.remove(&key);
    });

    set_status(&mut lib_state, &key, ImportStatus::Downloading);
    download_pdf_into_library(db, &mut lib_state, &paper, &id).await;
    lib_state.with_mut(|s| {
        s.import_status.remove(&key);
    });
}

/// Identity for a queued import, matching the search result list's `result_key`
/// so a row can look up its own status.
fn import_key(paper: &Paper) -> String {
    match paper.paper_id() {
        Some(pid) => pid.to_stored_string(),
        None => rotero_models::normalize_title(&paper.title),
    }
}

fn set_status(lib_state: &mut Signal<LibraryState>, key: &str, status: ImportStatus) {
    lib_state.with_mut(|s| {
        s.import_status.insert(key.to_string(), status);
    });
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
    /// Carried so queuing can mark the row immediately; `Signal` is `Copy`, so
    /// this keeps the handle `Copy` too.
    lib_state: Signal<LibraryState>,
}

impl ImportChannel {
    /// Queue a paper for import + OA download. Returns immediately; the work
    /// runs in the app-root scope and survives the caller unmounting.
    pub fn import(&self, paper: Paper) {
        // Marked queued before sending so the button changes on the click
        // itself, rather than whenever a slot happens to free up.
        let key = import_key(&paper);
        let mut lib_state = self.lib_state;
        lib_state.with_mut(|s| {
            s.import_status.insert(key, ImportStatus::Queued);
        });
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
            // Imports run concurrently but bounded. Awaiting each one inline
            // made the queue strictly serial, so a single slow or hanging
            // download blocked every paper behind it — the "import is stuck"
            // report. The cap keeps bulk imports polite to rate-limited
            // providers like Semantic Scholar.
            const MAX_CONCURRENT: usize = 4;
            let mut tasks: futures_util::stream::FuturesUnordered<_> = Default::default();

            loop {
                if tasks.len() >= MAX_CONCURRENT {
                    // At capacity: only drain, so back-pressure reaches the
                    // channel rather than growing an unbounded task set.
                    tasks.next().await;
                    continue;
                }

                tokio::select! {
                    req = rx.next() => match req {
                        Some(req) => {
                            let db = db.clone();
                            tasks.push(async move {
                                run_import(&db, lib_state, req.paper).await;
                            });
                        }
                        // Channel closed and nothing left to finish.
                        None if tasks.is_empty() => break,
                        None => {
                            while tasks.next().await.is_some() {}
                            break;
                        }
                    },
                    _ = tasks.next(), if !tasks.is_empty() => {}
                }
            }
        }
    });
    ImportChannel {
        inner: coro,
        lib_state,
    }
}

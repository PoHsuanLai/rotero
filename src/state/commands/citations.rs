//! One-time citation-graph population.
//!
//! Extracts every library PDF's external links, resolves each to a library paper
//! (by DOI/arXiv/URL), and records the resulting directed `citing → cited` edges
//! in `paper_citations`. Runs once per install, guarded by an `app_flags` row.

use super::PdfDocs;
use rotero_db::Database;

/// `app_flags` key marking the initial scan complete.
const SCAN_FLAG: &str = "citations_scanned";

/// Populate the citation graph if it hasn't been done on this install yet.
///
/// Best-effort and idempotent: individual paper failures are skipped, and the
/// completion flag is only set after a full pass so an interrupted run retries
/// next launch. Returns the number of citation edges inserted.
pub async fn scan_citations_if_needed(docs: &PdfDocs, db: &Database) -> usize {
    if matches!(db.get_app_flag(SCAN_FLAG).await, Ok(Some(_))) {
        return 0;
    }

    let papers = db.list_papers().await.unwrap_or_default();
    let mut inserted = 0usize;

    for paper in &papers {
        let (Some(citing_id), Some(rel)) = (paper.id.as_deref(), paper.links.pdf_path.as_deref())
        else {
            continue;
        };
        let full = db.resolve_pdf_path(rel);
        if !full.exists() {
            continue;
        }

        let Ok(links) = docs.extract_links(full.to_string_lossy().to_string()).await else {
            continue;
        };

        // Resolve each external link to a library paper and record the edge.
        // Dedup within this PDF so we don't hammer the DB with repeats.
        let mut seen = std::collections::HashSet::new();
        for link in &links {
            let rotero_pdf::LinkTarget::External { uri } = &link.target else {
                continue;
            };
            if !seen.insert(uri.clone()) {
                continue;
            }
            if let Ok(Some(cited)) = db.find_paper_by_link(uri).await
                && let Some(cited_id) = cited.id.as_deref()
                && cited_id != citing_id
                && db.insert_citation(citing_id, cited_id).await.is_ok()
            {
                inserted += 1;
            }
        }
    }

    let _ = db.set_app_flag(SCAN_FLAG, "1").await;
    tracing::info!("citation scan complete: {inserted} edges inserted");
    inserted
}

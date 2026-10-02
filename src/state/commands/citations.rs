//! Citation-graph population.
//!
//! For each library PDF this device has not already scanned at its current
//! content hash, extract external links. A link that resolves to a library
//! paper becomes a `paper_citations` edge. A link that parses as a DOI, arXiv
//! id, PMID, or ISBN and does not resolve becomes a `reference_stubs` row.
//! Anything else is dropped.
//!
//! The old install-wide `citations_scanned` flag is left in place and no longer
//! consulted. A library that finished that scan before a PDF existed would
//! otherwise never look again.

use super::PdfDocs;
use rotero_db::Database;
use rotero_models::CitationRecord;

/// Whether this device still needs to walk one PDF.
///
/// Equal hash means the file has not changed. An empty hash matches an empty
/// stored flag, so a paper with no hash is scanned once and retried when a
/// hash appears and the two stop matching.
pub fn citation_scan_due(stored: Option<&str>, sha: &str) -> bool {
    stored != Some(sha)
}

fn scan_flag_key(paper_id: &str) -> String {
    format!("citation_scan:{paper_id}")
}

/// Walk local PDFs whose content hash is not the one already recorded.
///
/// Best-effort: a paper whose links cannot be read is skipped and not flagged,
/// so the next launch tries it again. Returns how many links became a citation
/// or a stub.
pub async fn scan_citations_if_needed(docs: &PdfDocs, db: &Database) -> usize {
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

        let sha = db
            .pdf_sha256(citing_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let key = scan_flag_key(citing_id);
        let stored = db.get_app_flag(&key).await.ok().flatten();
        if !citation_scan_due(stored.as_deref(), &sha) {
            continue;
        }

        let Ok(links) = docs.extract_links(full.to_string_lossy().to_string()).await else {
            continue;
        };

        let mut seen = std::collections::HashSet::new();
        for link in &links {
            let rotero_pdf::LinkTarget::External { uri } = &link.target else {
                continue;
            };
            if !seen.insert(uri.clone()) {
                continue;
            }
            if matches!(
                db.note_external_citation(citing_id, uri).await,
                Ok(CitationRecord::Citation { .. } | CitationRecord::Stub { .. })
            ) {
                inserted += 1;
            }
        }

        let _ = db.set_app_flag(&key, &sha).await;
    }

    tracing::info!("citation scan complete: {inserted} links recorded");
    inserted
}

#[cfg(test)]
mod tests {
    use super::citation_scan_due;

    #[test]
    fn an_unchanged_hash_is_not_due() {
        assert!(!citation_scan_due(Some("abc"), "abc"));
    }

    #[test]
    fn a_new_hash_is_due() {
        assert!(citation_scan_due(None, "abc"));
        assert!(citation_scan_due(Some(""), "abc"));
        assert!(citation_scan_due(Some("old"), "abc"));
    }

    #[test]
    fn a_paper_with_no_hash_is_scanned_once() {
        assert!(citation_scan_due(None, ""));
        assert!(!citation_scan_due(Some(""), ""));
    }
}

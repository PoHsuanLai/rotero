//! Open a paper's PDF via the library path for MCP tools.

use rotero_pdf::Document;
use rotero_pdf::PdfError;

use super::RoteroMcp;

fn err(msg: impl std::fmt::Display) -> rmcp::ErrorData {
    rmcp::ErrorData::internal_error(msg.to_string(), None)
}

impl RoteroMcp {
    /// Resolve `paper_id` → absolute PDF path → open [`Document`].
    pub(super) async fn open_paper_pdf(
        &self,
        paper_id: &str,
    ) -> Result<(rotero_models::Paper, Document), rmcp::ErrorData> {
        let paper = self
            .db
            .get_paper_by_id(paper_id)
            .await
            .map_err(err)?
            .ok_or_else(|| err(format!("No paper found with ID {paper_id}")))?;
        let rel = paper
            .links
            .pdf_path
            .as_ref()
            .ok_or_else(|| err(format!("Paper {paper_id} has no attached PDF")))?;
        let abs = self.db.resolve_pdf_path(rel);
        let doc = Document::open(&abs).map_err(|e| {
            err(format!(
                "Failed to open PDF for {paper_id} at {}: {e}",
                abs.display()
            ))
        })?;
        Ok((paper, doc))
    }
}

/// Clamp a 1-based inclusive page range to `[1, total]`.
pub(super) fn clamp_page_range(
    page_start: Option<u32>,
    page_end: Option<u32>,
    total_pages: u32,
    default_span: u32,
) -> Result<(u32, u32), rmcp::ErrorData> {
    if total_pages == 0 {
        return Err(err("PDF has no pages"));
    }
    let start = page_start.unwrap_or(1).max(1).min(total_pages);
    let end = page_end
        .unwrap_or_else(|| start.saturating_add(default_span.saturating_sub(1)))
        .max(start)
        .min(total_pages);
    Ok((start, end))
}

/// Map a [`PdfError`] into MCP error data.
pub(super) fn pdf_err(e: PdfError) -> rmcp::ErrorData {
    err(e)
}

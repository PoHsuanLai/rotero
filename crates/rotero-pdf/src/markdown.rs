//! Markdown extraction via pdfrum's `markdown` feature.

use pdfrum::Document;

use crate::doc::PdfError;

/// GitHub-flavoured Markdown for a single page.
pub fn page_markdown(doc: &Document, page_index: u32) -> Result<String, PdfError> {
    let page_count = doc.page_count();
    if page_index >= page_count {
        return Err(PdfError::PageOutOfRange(page_index, page_count));
    }
    let page = doc.page(page_index)?;
    Ok(page.markdown())
}

/// Whole-document Markdown (`---` between pages).
pub fn document_markdown(doc: &Document) -> String {
    doc.markdown()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    #[test]
    fn page_markdown_returns_non_empty_for_fixture() {
        let doc = Document::open(fixture("basicapi.pdf")).expect("open");
        let md = page_markdown(&doc, 0).expect("page 0");
        assert!(!md.trim().is_empty(), "expected markdown text, got {md:?}");
    }

    #[test]
    fn document_markdown_covers_all_pages() {
        let doc = Document::open(fixture("basicapi.pdf")).expect("open");
        let md = document_markdown(&doc);
        assert!(!md.trim().is_empty());
        // Multi-page fixtures get `---` separators; single-page may not.
        if doc.page_count() > 1 {
            assert!(md.contains("---"), "expected page separators");
        }
    }
}

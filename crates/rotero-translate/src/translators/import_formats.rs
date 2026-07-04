//! Bibliography import formats: RIS, BibTeX, CSL-JSON, and NBIB. These wrap the
//! existing `rotero-bib` parsers behind the registry's import entry point,
//! producing [`ZoteroItem`]s so the import path shares one type with the web
//! translators. BibTeX's local PDF path is preserved as an attachment.

use rotero_bib::{ImportedPaper, import_bibtex, import_csl_json, import_nbib, import_ris};

use crate::TranslateError;
use crate::item::{ZoteroAttachment, ZoteroItem};

/// A recognized bibliography format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    Ris,
    BibTeX,
    CslJson,
    Nbib,
}

impl ImportFormat {
    /// Guess the format from the text's leading content.
    pub fn sniff(text: &str) -> Self {
        let t = text.trim_start();
        // RIS records start with a "TY  - " type tag.
        if t.starts_with("TY  -") || t.starts_with("TY - ") {
            return Self::Ris;
        }
        // NBIB (PubMed) records start with "PMID- ".
        if t.starts_with("PMID-") {
            return Self::Nbib;
        }
        // CSL-JSON is a JSON array or object.
        if t.starts_with('[') || t.starts_with('{') {
            return Self::CslJson;
        }
        // Default: BibTeX (@article{...}, @book{...}).
        Self::BibTeX
    }
}

/// Parse bibliography `text` in the given (or sniffed) format into items.
pub fn parse_import(text: &str, format: ImportFormat) -> Result<Vec<ZoteroItem>, TranslateError> {
    let items = match format {
        ImportFormat::Ris => import_ris(text)
            .map_err(TranslateError::Translation)?
            .into_iter()
            .map(ZoteroItem::from_paper)
            .collect(),
        ImportFormat::CslJson => import_csl_json(text)
            .map_err(TranslateError::Translation)?
            .into_iter()
            .map(ZoteroItem::from_paper)
            .collect(),
        ImportFormat::Nbib => import_nbib(text)
            .map_err(TranslateError::Translation)?
            .into_iter()
            .map(ZoteroItem::from_paper)
            .collect(),
        ImportFormat::BibTeX => import_bibtex(text)
            .map_err(TranslateError::Translation)?
            .into_iter()
            .map(imported_paper_to_item)
            .collect(),
    };
    Ok(items)
}

/// Convert a BibTeX [`ImportedPaper`] into a [`ZoteroItem`], preserving its
/// local `source_pdf` (a path relative to the `.bib` file) as an attachment.
fn imported_paper_to_item(imported: ImportedPaper) -> ZoteroItem {
    let mut item = ZoteroItem::from_paper(imported.paper);
    if let Some(pdf) = imported.source_pdf.filter(|s| !s.is_empty()) {
        item.attachments.push(ZoteroAttachment {
            title: "Full Text PDF".to_string(),
            path: pdf,
            mime_type: "application/pdf".to_string(),
            ..Default::default()
        });
    }
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_formats() {
        assert_eq!(ImportFormat::sniff("TY  - JOUR\nTI  - X\nER  -"), ImportFormat::Ris);
        assert_eq!(ImportFormat::sniff("PMID- 12345\nTI  - X"), ImportFormat::Nbib);
        assert_eq!(ImportFormat::sniff("[{\"title\":\"x\"}]"), ImportFormat::CslJson);
        assert_eq!(ImportFormat::sniff("  {\"title\":\"x\"}"), ImportFormat::CslJson);
        assert_eq!(ImportFormat::sniff("@article{k, title={X}}"), ImportFormat::BibTeX);
    }

    #[test]
    fn parses_ris() {
        let ris = "TY  - JOUR\nTI  - A Test Paper\nAU  - Doe, John\nPY  - 2020\nER  -\n";
        let items = parse_import(ris, ImportFormat::Ris).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "A Test Paper");
    }

    #[test]
    fn bibtex_source_pdf_becomes_attachment() {
        let bib = "@article{k,\n  title={X},\n  author={Doe, John},\n  file={papers/x.pdf}\n}";
        let items = parse_import(bib, ImportFormat::BibTeX).unwrap();
        assert_eq!(items.len(), 1);
        let has_local_pdf = items[0]
            .attachments
            .iter()
            .any(|a| a.path.ends_with("x.pdf"));
        assert!(has_local_pdf, "expected local PDF attachment from bibtex file field");
    }
}

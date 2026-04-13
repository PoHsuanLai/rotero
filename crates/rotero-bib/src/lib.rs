//! Bibliography import/export for Rotero.
//!
//! Supports BibTeX, RIS, CSL-JSON, and NBIB (PubMed/MEDLINE) formats for
//! importing papers, BibTeX export, and citation formatting via hayagriva
//! with 14 CSL styles.

/// Citation formatting using hayagriva and CSL styles.
pub mod citation;
/// BibTeX export from `Paper` structs.
pub mod export;
/// BibTeX import into `ImportedPaper` structs.
pub mod import_bibtex;
/// CSL-JSON import (Zotero/Mendeley standard format).
pub mod import_csl;
/// NBIB (PubMed/MEDLINE) import.
pub mod import_nbib;
/// RIS import.
pub mod import_ris;

pub use citation::{
    AVAILABLE_STYLES, format_bibliography, format_bibliography_entries, format_citation,
    format_inline_citations,
};
pub use export::{export_bibtex, generate_cite_key, generate_unique_cite_key};
/// Re-export of hayagriva's `ArchivedStyle` for selecting citation styles.
pub use hayagriva::archive::ArchivedStyle;
pub use import_bibtex::{ImportedPaper, import_bibtex};
pub use import_csl::import_csl_json;
pub use import_nbib::import_nbib;
pub use import_ris::import_ris;

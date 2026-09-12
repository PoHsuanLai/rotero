//! PDF rendering, annotation writing, and text extraction for the Rotero paper reader.
//!
//! Built on `pdfrum` for rendering, text extraction, and annotation writing.
//! Documents are opened via [`DocCache`] (or `pdfrum::Document::open`); operations
//! are free functions that take `&Document`.

/// Annotation writing to PDF files via pdfrum `DocEdit`.
pub mod annotations;
/// Document cache and PDF operations (render, outline, links, annotations).
pub mod doc;
/// Text extraction, font detection, and full-text search over PDF pages.
pub mod text_extract;

pub use annotations::write_annotations;
pub use doc::{
    BookmarkEntry, DocCache, ExtractedAnnotation, ExtractedLink, LinkTarget, PdfDocumentInfo,
    PdfError, RenderedPage, extract_annotations, extract_links, load_document,
    open_and_render_initial, outline, page_dimensions, render_pages, render_thumbnails,
};
pub use text_extract::{
    PageTextData, PdfDocMetadata, SearchMatch, SelectionMarkup, TextSegment, group_into_lines,
    search_in_document, selection_markup, text_block_at,
};

/// Re-export so app code can create per-call render sessions without depending on pdfrum directly.
pub use pdfrum::RenderSession;

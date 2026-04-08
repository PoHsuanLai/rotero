//! PDF rendering, annotation writing, and text extraction for the Rotero paper reader.
//!
//! Built on `pdfium-render` for rendering/text extraction and `lopdf` for writing annotations.

/// Annotation writing to PDF files via lopdf.
pub mod annotations;
/// PDF rendering engine, document loading, and annotation extraction via pdfium-render.
pub mod renderer;
/// Text extraction, font detection, and full-text search over PDF pages.
pub mod text_extract;

pub use annotations::write_annotations;
pub use renderer::{
    BookmarkEntry, ExtractedAnnotation, PdfDocumentInfo, PdfEngine, PdfError, RenderedPage,
};
pub use text_extract::{PageTextData, PdfDocMetadata, SearchMatch, TextSegment, group_into_lines};

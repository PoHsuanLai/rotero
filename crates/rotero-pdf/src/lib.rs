//! PDF rendering, annotation writing, and text extraction for the Rotero paper reader.
//!
//! Built on `pdfrum` for rendering, text extraction, and annotation writing.
//! Documents are opened via [`DocCache`] (or `pdfrum::Document::open`); operations
//! are free functions that take `&Document`.

/// Annotation writing to PDF files via pdfrum `DocEdit`.
pub mod annotations;
/// Document cache and PDF operations (render, outline, links, annotations).
pub mod doc;
/// Page figure / image listing and PNG extraction.
pub mod images;
/// Markdown extraction (pdfrum `markdown` feature).
pub mod markdown;
/// Text extraction, font detection, and full-text search over PDF pages.
pub mod text_extract;

pub use annotations::write_annotations;
pub use doc::{
    BookmarkEntry, DocCache, ExtractedAnnotation, ExtractedLink, LinkTarget, PdfDocumentInfo,
    PdfError, RenderedPage, display_page_number, extract_annotations, extract_links, load_document,
    open_and_render_initial, open_and_render_initial_with, open_and_render_initial_with_theme,
    outline, page_dimensions, page_labels, render_options_for, render_pages, render_pages_with,
    render_thumbnails,
};
pub use images::{PageFigure, extract_page_image_png, list_page_images};
pub use markdown::{document_markdown, page_markdown};
pub use text_extract::{
    ClickSelectMode, PageTextData, PdfDocMetadata, SearchMatch, SelectionMarkup, TextSegment,
    group_into_lines, search_in_document, selection_at_point, selection_at_point_from_text,
    selection_markup, selection_markup_from_text, text_block_at,
};

/// Re-export so app code can create per-call render sessions without depending on pdfrum directly.
pub use pdfrum::RenderSession;

//! PDF text extraction: font detection, segment extraction, and search.

pub mod font;
pub mod search;
pub mod segments;

use std::sync::Arc;
use serde::{Deserialize, Serialize};

/// A single text segment with its position in pixel coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSegment {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    /// CSS font-family string derived from the PDF font.
    pub font_family: String,
    /// CSS font-weight (e.g. "normal", "bold", "700").
    pub font_weight: String,
    /// CSS font-style ("normal" or "italic").
    pub font_style: String,
}

/// All extracted text segments for a single page.
/// Segments are wrapped in `Arc` so that cloning `PageTextData` (which happens
/// frequently during Dioxus render cycles) is cheap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextData {
    pub page_index: u32,
    pub segments: Arc<Vec<TextSegment>>,
}

// Re-exports for public API
pub use search::{SearchMatch, group_into_lines, search_in_text_data};
pub use segments::{
    PdfDocMetadata, extract_doc_metadata, extract_page_text, extract_pages_text, extract_raw_text,
};

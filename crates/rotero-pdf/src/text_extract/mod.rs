//! PDF text extraction: font detection, segment extraction, and search.

/// Font name heuristics for CSS weight, style, and family mapping.
pub mod font;
/// Full-text search over extracted page text data.
pub mod search;
/// Per-page text segment extraction and document metadata reading.
pub mod segments;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A single text segment with its position in pixel coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSegment {
    /// The text content of this segment.
    pub text: String,
    /// Left edge in pixels.
    pub x: f64,
    /// Top edge in pixels.
    pub y: f64,
    /// Width in pixels.
    pub width: f64,
    /// Height in pixels.
    pub height: f64,
    /// Font size in pixels.
    pub font_size: f64,
    /// CSS font-family string with generic fallback.
    pub font_family: String,
    /// CSS font-weight (e.g. `"normal"`, `"bold"`, `"300"`).
    pub font_weight: String,
    /// CSS font-style (`"normal"` or `"italic"`).
    pub font_style: String,
}

/// Segments wrapped in `Arc` so cloning during Dioxus render cycles is cheap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextData {
    /// Zero-based page number.
    pub page_index: u32,
    /// Extracted text segments for this page.
    pub segments: Arc<Vec<TextSegment>>,
}

pub use search::{SearchMatch, group_into_lines, search_in_text_data};
pub use segments::{
    PdfDocMetadata, extract_doc_metadata, extract_page_text, extract_pages_text, extract_raw_text,
};

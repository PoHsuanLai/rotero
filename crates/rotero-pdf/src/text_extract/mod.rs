//! PDF text extraction: font detection, segment extraction, and search.

pub mod font;
pub mod search;
pub mod segments;

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A single text segment with its position in pixel coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSegment {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
    pub font_family: String,
    pub font_weight: String,
    pub font_style: String,
}

/// Segments wrapped in `Arc` so cloning during Dioxus render cycles is cheap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextData {
    pub page_index: u32,
    pub segments: Arc<Vec<TextSegment>>,
}

pub use search::{SearchMatch, group_into_lines, search_in_text_data};
pub use segments::{
    PdfDocMetadata, extract_doc_metadata, extract_page_text, extract_pages_text, extract_raw_text,
};

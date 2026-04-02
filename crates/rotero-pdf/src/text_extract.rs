use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PdfError;

/// A single text segment with its position in pixel coordinates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextSegment {
    pub text: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub font_size: f64,
}

/// All extracted text segments for a single page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextData {
    pub page_index: u32,
    pub segments: Vec<TextSegment>,
}

/// Extract text segments with bounding boxes from a single PDF page.
///
/// Coordinates are returned in pixel space (matching the rendered image at the given scale).
/// PDF coordinates (origin bottom-left) are converted to screen coordinates (origin top-left).
pub fn extract_page_text(
    pdfium: &Pdfium,
    pdf_path: &str,
    page_index: u32,
    scale: f32,
) -> Result<PageTextData, PdfError> {
    let document = pdfium.load_pdf_from_file(pdf_path, None)?;
    let page = document
        .pages()
        .get(page_index as u16)
        .map_err(|e| PdfError::RenderError(e.to_string()))?;

    let page_height_pts = page.height().value;
    let text = page.text().map_err(|e| PdfError::RenderError(e.to_string()))?;

    let mut segments = Vec::new();

    for segment in text.segments().iter() {
        let seg_text = segment.text();
        if seg_text.trim().is_empty() {
            continue;
        }

        #[allow(deprecated)]
        let bounds = segment.bounds();

        // PDF coords: origin at bottom-left, y increases upward
        // Screen coords: origin at top-left, y increases downward
        let left_pts = bounds.left().value;
        let top_pts = bounds.top().value;
        let right_pts = bounds.right().value;
        let bottom_pts = bounds.bottom().value;

        // Convert to pixel coordinates
        let x = (left_pts * scale) as f64;
        let y = ((page_height_pts - top_pts) * scale) as f64;
        let width = ((right_pts - left_pts) * scale) as f64;
        let height = ((top_pts - bottom_pts) * scale) as f64;

        // Estimate font size from segment height
        let font_size = height;

        if width > 0.0 && height > 0.0 {
            segments.push(TextSegment {
                text: seg_text,
                x,
                y,
                width,
                height,
                font_size,
            });
        }
    }

    Ok(PageTextData {
        page_index,
        segments,
    })
}

/// Extract text segments from multiple pages in batch.
pub fn extract_pages_text(
    pdfium: &Pdfium,
    pdf_path: &str,
    start: u32,
    count: u32,
    scale: f32,
) -> Result<Vec<PageTextData>, PdfError> {
    let document = pdfium.load_pdf_from_file(pdf_path, None)?;
    let page_count = document.pages().len() as u32;
    let end = (start + count).min(page_count);

    let mut results = Vec::new();

    for page_index in start..end {
        let page = document
            .pages()
            .get(page_index as u16)
            .map_err(|e| PdfError::RenderError(e.to_string()))?;

        let page_height_pts = page.height().value;
        let text = match page.text() {
            Ok(t) => t,
            Err(_) => {
                // Some pages may not have extractable text
                results.push(PageTextData {
                    page_index,
                    segments: Vec::new(),
                });
                continue;
            }
        };

        let mut segments = Vec::new();

        for segment in text.segments().iter() {
            let seg_text = segment.text();
            if seg_text.trim().is_empty() {
                continue;
            }

            #[allow(deprecated)]
            let bounds = segment.bounds();

            let left_pts = bounds.left().value;
            let top_pts = bounds.top().value;
            let right_pts = bounds.right().value;
            let bottom_pts = bounds.bottom().value;

            let x = (left_pts * scale) as f64;
            let y = ((page_height_pts - top_pts) * scale) as f64;
            let width = ((right_pts - left_pts) * scale) as f64;
            let height = ((top_pts - bottom_pts) * scale) as f64;
            let font_size = height;

            if width > 0.0 && height > 0.0 {
                segments.push(TextSegment {
                    text: seg_text,
                    x,
                    y,
                    width,
                    height,
                    font_size,
                });
            }
        }

        results.push(PageTextData {
            page_index,
            segments,
        });
    }

    Ok(results)
}

/// A search match with its location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub page_index: u32,
    /// Bounding rectangles for the match (x, y, width, height in pixels).
    pub bounds: Vec<(f64, f64, f64, f64)>,
    pub matched_text: String,
}

/// Search for text across all pages using already-extracted text data.
/// This operates on in-memory PageTextData, no PDF file access needed.
pub fn search_in_text_data(
    text_data: &[PageTextData],
    query: &str,
) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    for page_data in text_data {
        for segment in &page_data.segments {
            let seg_lower = segment.text.to_lowercase();
            if seg_lower.contains(&query_lower) {
                // The entire segment bounds serve as the match highlight
                matches.push(SearchMatch {
                    page_index: page_data.page_index,
                    bounds: vec![(segment.x, segment.y, segment.width, segment.height)],
                    matched_text: segment.text.clone(),
                });
            }
        }
    }

    matches
}

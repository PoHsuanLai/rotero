use pdfium_render::prelude::*;
use serde::{Deserialize, Serialize};

use crate::PdfError;

/// Detect CSS font-weight from the PDF font name.
fn detect_font_weight(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("bold") || lower.contains("-bd") || lower.contains("demi") {
        "bold".to_string()
    } else if lower.contains("light") || lower.contains("thin") {
        "300".to_string()
    } else if lower.contains("black") || lower.contains("heavy") {
        "900".to_string()
    } else if lower.contains("medium") && !lower.contains("mediumitalic") {
        "500".to_string()
    } else {
        "normal".to_string()
    }
}

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
}

/// Map a PDF font name to a CSS font-family string.
fn pdf_font_to_css(name: &str, is_serif: bool) -> String {
    let lower = name.to_lowercase();

    // Common PDF font name patterns
    if lower.contains("times") || lower.contains("palatino") || lower.contains("garamond") {
        return format!("\"{name}\", serif");
    }
    if lower.contains("helvetica") || lower.contains("arial") || lower.contains("opensans") {
        return format!("\"{name}\", sans-serif");
    }
    if lower.contains("courier") || lower.contains("consolas") || lower.contains("mono") {
        return format!("\"{name}\", monospace");
    }
    if lower.contains("symbol") || lower.contains("zapf") {
        return format!("\"{name}\", symbol");
    }
    if lower.contains("cmbx") || lower.contains("cmr") || lower.contains("cmmi")
        || lower.contains("cmsy") || lower.contains("cmex") || lower.contains("cmti")
    {
        // Computer Modern (LaTeX) — serif
        return format!("\"{name}\", serif");
    }

    // Fall back to font descriptor flags
    if is_serif {
        format!("\"{name}\", serif")
    } else {
        format!("\"{name}\", sans-serif")
    }
}

/// All extracted text segments for a single page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextData {
    pub page_index: u32,
    pub segments: Vec<TextSegment>,
}

/// Extract text segments with bounding boxes from a single PDF page.
///
/// `img_width`/`img_height` are the actual rendered image dimensions in pixels.
/// Coordinates are returned in pixel space matching those dimensions.
/// PDF coordinates (origin bottom-left) are converted to screen coordinates (origin top-left).
pub fn extract_page_text(
    pdfium: &Pdfium,
    pdf_path: &str,
    page_index: u32,
    img_width: u32,
    img_height: u32,
) -> Result<PageTextData, PdfError> {
    let document = pdfium.load_pdf_from_file(pdf_path, None)?;
    let page = document
        .pages()
        .get(page_index as u16)
        .map_err(|e| PdfError::RenderError(e.to_string()))?;

    let page_width_pts = page.width().value;
    let page_height_pts = page.height().value;

    // Scale factors from PDF points to actual image pixels
    let scale_x = img_width as f64 / page_width_pts as f64;
    let scale_y = img_height as f64 / page_height_pts as f64;

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
        let left_pts = bounds.left().value as f64;
        let top_pts = bounds.top().value as f64;
        let right_pts = bounds.right().value as f64;
        let bottom_pts = bounds.bottom().value as f64;

        // Convert to pixel coordinates using actual image dimensions
        let x = left_pts * scale_x;
        let y = (page_height_pts as f64 - top_pts) * scale_y;
        let width = (right_pts - left_pts) * scale_x;
        let height = (top_pts - bottom_pts) * scale_y;

        let font_size = height;

        // Get font info from the first character in this segment
        let (font_family, font_weight) = segment.chars()
            .ok()
            .and_then(|chars| {
                let first_char = chars.iter().next()?;
                let name = first_char.font_name();
                let is_serif = first_char.font_is_serif();
                let weight = detect_font_weight(&name);
                if name.is_empty() {
                    None
                } else {
                    Some((pdf_font_to_css(&name, is_serif), weight))
                }
            })
            .unwrap_or_else(|| ("sans-serif".to_string(), "normal".to_string()));

        if width > 0.0 && height > 0.0 {
            segments.push(TextSegment {
                text: seg_text,
                x,
                y,
                width,
                height,
                font_size,
                font_family,
                font_weight,
            });
        }
    }

    Ok(PageTextData {
        page_index,
        segments,
    })
}

/// Extract text segments from multiple pages in batch.
/// `page_dims` maps page_index to (img_width, img_height) of the rendered image.
pub fn extract_pages_text(
    pdfium: &Pdfium,
    pdf_path: &str,
    page_dims: &[(u32, u32, u32)], // (page_index, img_width, img_height)
) -> Result<Vec<PageTextData>, PdfError> {
    let document = pdfium.load_pdf_from_file(pdf_path, None)?;

    let mut results = Vec::new();

    for &(page_index, img_width, img_height) in page_dims {
        let page = document
            .pages()
            .get(page_index as u16)
            .map_err(|e| PdfError::RenderError(e.to_string()))?;

        let page_width_pts = page.width().value;
        let page_height_pts = page.height().value;
        let scale_x = img_width as f64 / page_width_pts as f64;
        let scale_y = img_height as f64 / page_height_pts as f64;

        let text = match page.text() {
            Ok(t) => t,
            Err(_) => {
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

            let left_pts = bounds.left().value as f64;
            let top_pts = bounds.top().value as f64;
            let right_pts = bounds.right().value as f64;
            let bottom_pts = bounds.bottom().value as f64;

            let x = left_pts * scale_x;
            let y = (page_height_pts as f64 - top_pts) * scale_y;
            let width = (right_pts - left_pts) * scale_x;
            let height = (top_pts - bottom_pts) * scale_y;
            let font_size = height;

            let (font_family, font_weight) = segment.chars()
                .ok()
                .and_then(|chars| {
                    let first_char = chars.iter().next()?;
                    let name = first_char.font_name();
                    let is_serif = first_char.font_is_serif();
                    let weight = detect_font_weight(&name);
                    if name.is_empty() { None } else { Some((pdf_font_to_css(&name, is_serif), weight)) }
                })
                .unwrap_or_else(|| ("sans-serif".to_string(), "normal".to_string()));

            if width > 0.0 && height > 0.0 {
                segments.push(TextSegment {
                    text: seg_text,
                    x,
                    y,
                    width,
                    height,
                    font_size,
                    font_family,
                    font_weight,
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

/// Document-level metadata extracted from PDF properties (XMP / DocInfo).
#[derive(Debug, Clone, Default)]
pub struct PdfDocMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
}

/// Extract raw text content from specified pages (no position data).
/// Returns a Vec of (page_index, text_string) pairs.
pub fn extract_raw_text(
    pdfium: &Pdfium,
    pdf_path: &str,
    page_indices: &[u32],
) -> Result<Vec<(u32, String)>, PdfError> {
    let document = pdfium.load_pdf_from_file(pdf_path, None)?;
    let mut results = Vec::new();
    for &idx in page_indices {
        let page = match document.pages().get(idx as u16) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let text = match page.text() {
            Ok(t) => t.all(),
            Err(_) => String::new(),
        };
        results.push((idx, text));
    }
    Ok(results)
}

/// Extract document-level metadata (title, author, subject) from PDF properties.
pub fn extract_doc_metadata(
    pdfium: &Pdfium,
    pdf_path: &str,
) -> Result<PdfDocMetadata, PdfError> {
    use pdfium_render::prelude::PdfDocumentMetadataTagType;

    let document = pdfium.load_pdf_from_file(pdf_path, None)?;
    let metadata = document.metadata();

    let get = |tag: PdfDocumentMetadataTagType| -> Option<String> {
        metadata.get(tag).map(|t| t.value().to_string()).filter(|s| !s.trim().is_empty())
    };

    Ok(PdfDocMetadata {
        title: get(PdfDocumentMetadataTagType::Title),
        author: get(PdfDocumentMetadataTagType::Author),
        subject: get(PdfDocumentMetadataTagType::Subject),
    })
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

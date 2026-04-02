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

/// Detect CSS font-style from the PDF font name and italic flag.
fn detect_font_style(name: &str, is_italic_flag: bool) -> String {
    if is_italic_flag {
        return "italic".to_string();
    }
    let lower = name.to_lowercase();
    if lower.contains("italic") || lower.contains("oblique")
        || lower.contains("-it") || lower.contains("slant")
        // LaTeX italic fonts
        || lower.contains("cmti") || lower.contains("cmmi")
    {
        "italic".to_string()
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
    /// CSS font-style ("normal" or "italic").
    pub font_style: String,
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

    // Character-level extraction: group consecutive chars by font into runs
    let all_chars = text.chars();

    let mut segments = Vec::new();

    // Current run state
    struct Run {
        text: String,
        font_name: String,
        is_italic: bool,
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
        font_size_pts: f32,
    }

    impl Run {
        fn new() -> Self {
            Self {
                text: String::new(),
                font_name: String::new(),
                is_italic: false,
                left: f64::MAX, top: f64::MIN,
                right: f64::MIN, bottom: f64::MAX,
                font_size_pts: 0.0,
            }
        }

        fn reset_bounds(&mut self) {
            self.left = f64::MAX; self.top = f64::MIN;
            self.right = f64::MIN; self.bottom = f64::MAX;
            self.font_size_pts = 0.0;
        }

        fn flush(&mut self, segments: &mut Vec<TextSegment>, scale_x: f64, scale_y: f64, page_height_pts: f32) {
            if self.text.trim().is_empty() {
                self.text.clear();
                return;
            }

            let x = self.left * scale_x;
            let y = (page_height_pts as f64 - self.top) * scale_y;
            let width = (self.right - self.left) * scale_x;
            let height = (self.top - self.bottom) * scale_y;
            let font_size = self.font_size_pts as f64 * scale_y;
            let is_serif = self.font_name.to_lowercase().contains("times")
                || self.font_name.to_lowercase().contains("serif")
                || self.font_name.to_lowercase().contains("cm");
            let font_family = if self.font_name.is_empty() {
                "sans-serif".to_string()
            } else {
                pdf_font_to_css(&self.font_name, is_serif)
            };
            let font_weight = detect_font_weight(&self.font_name);
            let font_style = detect_font_style(&self.font_name, self.is_italic);

            if width > 0.0 && height > 0.0 {
                segments.push(TextSegment {
                    text: std::mem::take(&mut self.text),
                    x, y, width, height, font_size, font_family, font_weight, font_style,
                });
            } else {
                self.text.clear();
            }
        }
    }

    let mut run = Run::new();

    for ch in all_chars.iter() {
        let c = match ch.unicode_char() {
            Some(c) => c,
            None => continue,
        };

        if c.is_control() && c != ' ' {
            if c == '\n' || c == '\r' {
                run.flush(&mut segments, scale_x, scale_y, page_height_pts);
                run.reset_bounds();
            }
            continue;
        }

        let font_name = ch.font_name();
        let is_italic = ch.font_is_italic() || detect_font_style(&font_name, false) == "italic";
        let font_size_pts = ch.scaled_font_size().value;

        // Split run on font name or italic change
        if !run.text.is_empty() && (font_name != run.font_name || is_italic != run.is_italic) {
            run.flush(&mut segments, scale_x, scale_y, page_height_pts);
            run.reset_bounds();
        }

        if let Ok(bounds) = ch.loose_bounds() {
            #[allow(deprecated)]
            {
                let l = bounds.left().value as f64;
                let t = bounds.top().value as f64;
                let r = bounds.right().value as f64;
                let b = bounds.bottom().value as f64;
                run.left = run.left.min(l);
                run.top = run.top.max(t);
                run.right = run.right.max(r);
                run.bottom = run.bottom.min(b);
            }
        }

        run.text.push(c);
        run.font_name = font_name;
        run.is_italic = is_italic;
        run.font_size_pts = font_size_pts;
    }

    run.flush(&mut segments, scale_x, scale_y, page_height_pts);

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
    let mut results = Vec::new();
    for &(page_index, img_width, img_height) in page_dims {
        match extract_page_text(pdfium, pdf_path, page_index, img_width, img_height) {
            Ok(data) => results.push(data),
            Err(_) => results.push(PageTextData { page_index, segments: Vec::new() }),
        }
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

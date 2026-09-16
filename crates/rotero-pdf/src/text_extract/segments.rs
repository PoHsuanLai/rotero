use std::sync::Arc;

use pdfrum::Document;

use crate::PdfError;

use super::PageTextData;
use super::TextSegment;
use super::font::{detect_font_style, detect_font_weight, pdf_font_to_css};

/// Coordinates are returned in pixel space matching `img_width`/`img_height`.
/// PDF coordinates (origin bottom-left) are converted to screen coordinates (origin top-left).
pub fn extract_page_text(
    doc: &Document,
    page_index: u32,
    img_width: u32,
    img_height: u32,
) -> Result<PageTextData, PdfError> {
    extract_page_text_from_doc(doc, page_index, img_width, img_height)
}

/// Extracts text for many pages from an already-open document.
pub fn extract_pages_text(
    doc: &Document,
    page_dims: &[(u32, u32, u32)], // (page_index, img_width, img_height)
) -> Result<Vec<PageTextData>, PdfError> {
    let mut results = Vec::with_capacity(page_dims.len());
    for &(page_index, img_width, img_height) in page_dims {
        match extract_page_text_from_doc(doc, page_index, img_width, img_height) {
            Ok(data) => results.push(data),
            Err(_) => results.push(PageTextData {
                page_index,
                segments: Arc::new(Vec::new()),
            }),
        }
    }
    Ok(results)
}

fn extract_page_text_from_doc(
    document: &Document,
    page_index: u32,
    img_width: u32,
    img_height: u32,
) -> Result<PageTextData, PdfError> {
    let page = document.page(page_index)?;
    let page_width_pts = page.width();
    let page_height_pts = page.height();

    let scale_x = f64::from(img_width) / page_width_pts;
    let scale_y = f64::from(img_height) / page_height_pts;

    let text = page.text();
    let mut segments = Vec::new();

    for word in text.words() {
        if word.text.trim().is_empty() {
            continue;
        }

        let rect = word.rect;
        let font_size_pts = word.size;
        let font_size = font_size_pts * scale_y;
        let x = rect.x0 * scale_x;
        let width = rect.width() * scale_x;
        // PDF y-up → screen y-down: top of the glyph box.
        let y = (page_height_pts - rect.y1) * scale_y;
        let height = rect.height().abs() * scale_y;

        let font_name = word.font.as_deref().unwrap_or("");
        let is_serif = {
            let lower = font_name.to_lowercase();
            lower.contains("times") || lower.contains("serif") || lower.contains("cm")
        };
        let font_family = if font_name.is_empty() {
            "sans-serif".to_string()
        } else {
            pdf_font_to_css(font_name, is_serif)
        };
        let font_weight = detect_font_weight(font_name).to_string();
        let font_style = detect_font_style(font_name, false).to_string();

        let char_count = word.text.chars().count() as f64;
        let expected_width = font_size * char_count * 0.8;
        let reasonable =
            width > 0.0 && height > 0.0 && (expected_width < 1.0 || width < expected_width * 3.0);

        if !reasonable {
            continue;
        }

        // Trailing space so the browser includes word separators when
        // selecting/copying text from the virtual text layer.
        let mut text_out = word.text;
        if !text_out.ends_with(' ') {
            text_out.push(' ');
        }

        segments.push(TextSegment {
            text: text_out,
            x,
            y,
            width,
            height: if height > 0.0 { height } else { font_size },
            font_size,
            font_family,
            font_weight,
            font_style,
        });
    }

    Ok(PageTextData {
        page_index,
        segments: Arc::new(segments),
    })
}

/// Extract raw text content from specified pages (no position data).
pub fn extract_raw_text(
    doc: &Document,
    page_indices: &[u32],
) -> Result<Vec<(u32, String)>, PdfError> {
    let mut results = Vec::new();
    for &idx in page_indices {
        let Ok(page) = doc.page(idx) else {
            continue;
        };
        let text = page.text().to_string();
        results.push((idx, text));
    }
    Ok(results)
}

/// Title, author, and subject metadata read from a PDF's document info dictionary.
#[derive(Debug, Clone, Default)]
pub struct PdfDocMetadata {
    /// Document title, if present.
    pub title: Option<String>,
    /// Document author, if present.
    pub author: Option<String>,
    /// Document subject or description, if present.
    pub subject: Option<String>,
}

/// Reads document-level metadata (title, author, subject) from the PDF.
pub fn extract_doc_metadata(doc: &Document) -> PdfDocMetadata {
    let metadata = doc.metadata();
    let nonempty = |s: Option<String>| s.filter(|v| !v.trim().is_empty());
    PdfDocMetadata {
        title: nonempty(metadata.title),
        author: nonempty(metadata.author),
        subject: nonempty(metadata.subject),
    }
}

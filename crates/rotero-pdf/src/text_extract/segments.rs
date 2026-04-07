//! TextSegment extraction and PageTextData building from PDF documents.

use std::sync::Arc;

use pdfium_render::prelude::*;

use crate::PdfError;

use super::font::{detect_font_style, detect_font_weight, pdf_font_to_css};
use super::TextSegment;
use super::PageTextData;

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
    let bytes = std::fs::read(pdf_path)
        .map_err(|e| PdfError::RenderError(format!("Failed to read {pdf_path}: {e}")))?;
    let document = pdfium.load_pdf_from_byte_vec(bytes, None)?;
    extract_page_text_from_doc(&document, page_index, img_width, img_height)
}

/// Extract text segments from multiple pages in batch.
/// Opens the document once and extracts all pages, avoiding repeated file I/O.
/// `page_dims` maps page_index to (img_width, img_height) of the rendered image.
pub fn extract_pages_text(
    pdfium: &Pdfium,
    pdf_path: &str,
    page_dims: &[(u32, u32, u32)], // (page_index, img_width, img_height)
) -> Result<Vec<PageTextData>, PdfError> {
    let bytes = std::fs::read(pdf_path)
        .map_err(|e| PdfError::RenderError(format!("Failed to read {pdf_path}: {e}")))?;
    let document = pdfium.load_pdf_from_byte_vec(bytes, None)?;
    let mut results = Vec::with_capacity(page_dims.len());
    for &(page_index, img_width, img_height) in page_dims {
        match extract_page_text_from_doc(&document, page_index, img_width, img_height) {
            Ok(data) => results.push(data),
            Err(_) => results.push(PageTextData {
                page_index,
                segments: Arc::new(Vec::new()),
            }),
        }
    }
    Ok(results)
}

/// Extract text from a single page of an already-opened document.
fn extract_page_text_from_doc(
    document: &PdfDocument,
    page_index: u32,
    img_width: u32,
    img_height: u32,
) -> Result<PageTextData, PdfError> {
    let page = document
        .pages()
        .get(page_index as u16)
        .map_err(|e| PdfError::RenderError(e.to_string()))?;

    let page_width_pts = page.width().value;
    let page_height_pts = page.height().value;

    let scale_x = img_width as f64 / page_width_pts as f64;
    let scale_y = img_height as f64 / page_height_pts as f64;

    let text = page
        .text()
        .map_err(|e| PdfError::RenderError(e.to_string()))?;
    let all_chars = text.chars();

    let mut segments = Vec::new();

    struct Run {
        text: String,
        font_name: String,
        is_italic: bool,
        left: f64,
        origin_y: f64,
        right: f64,
        font_size_pts: f32,
        has_origin_y: bool,
    }

    impl Run {
        fn new() -> Self {
            Self {
                text: String::new(),
                font_name: String::new(),
                is_italic: false,
                left: f64::MAX,
                origin_y: 0.0,
                right: f64::MIN,
                font_size_pts: 0.0,
                has_origin_y: false,
            }
        }

        fn reset_bounds(&mut self) {
            self.left = f64::MAX;
            self.origin_y = 0.0;
            self.right = f64::MIN;
            self.font_size_pts = 0.0;
            self.has_origin_y = false;
        }

        fn flush(
            &mut self,
            segments: &mut Vec<TextSegment>,
            scale_x: f64,
            scale_y: f64,
            page_height_pts: f32,
        ) {
            if self.text.trim().is_empty() {
                self.text.clear();
                return;
            }

            let font_size = self.font_size_pts as f64 * scale_y;
            let x = self.left * scale_x;
            let width = (self.right - self.left) * scale_x;
            let y = if self.has_origin_y {
                let ascent_pts = self.font_size_pts as f64 * 0.8;
                let top_pts = self.origin_y + ascent_pts;
                (page_height_pts as f64 - top_pts) * scale_y
            } else {
                0.0
            };
            let height = font_size;

            let is_serif = self.font_name.to_lowercase().contains("times")
                || self.font_name.to_lowercase().contains("serif")
                || self.font_name.to_lowercase().contains("cm");
            let font_family = if self.font_name.is_empty() {
                "sans-serif".to_string()
            } else {
                pdf_font_to_css(&self.font_name, is_serif)
            };
            let font_weight = detect_font_weight(&self.font_name).to_string();
            let font_style = detect_font_style(&self.font_name, self.is_italic).to_string();

            let char_count = self.text.chars().count() as f64;
            let expected_width = font_size * char_count * 0.8;
            let reasonable = width > 0.0
                && height > 0.0
                && (expected_width < 1.0 || width < expected_width * 3.0);

            if reasonable {
                segments.push(TextSegment {
                    text: std::mem::take(&mut self.text),
                    x,
                    y,
                    width,
                    height,
                    font_size,
                    font_family,
                    font_weight,
                    font_style,
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

        if c == '\n' || c == '\r' {
            run.flush(&mut segments, scale_x, scale_y, page_height_pts);
            run.reset_bounds();
            continue;
        }

        if c.is_control() {
            continue;
        }

        if c.is_whitespace() {
            run.flush(&mut segments, scale_x, scale_y, page_height_pts);
            // Append trailing space to the last emitted segment so the
            // browser includes word separators when selecting/copying text
            // from the virtual text layer.
            if let Some(last) = segments.last_mut() {
                if !last.text.ends_with(' ') {
                    last.text.push(' ');
                }
            }
            run.reset_bounds();
            continue;
        }

        let font_name = ch.font_name();
        let is_italic = ch.font_is_italic() || detect_font_style(&font_name, false) == "italic";
        let font_size_pts = ch.scaled_font_size().value;

        if !run.text.is_empty() && (font_name != run.font_name || is_italic != run.is_italic) {
            run.flush(&mut segments, scale_x, scale_y, page_height_pts);
            run.reset_bounds();
        }

        if let Ok((ox, oy)) = ch.origin() {
            let ox = ox.value as f64;
            let oy = oy.value as f64;

            if !run.text.is_empty() && run.right > f64::MIN && ox < run.left - font_size_pts as f64
            {
                run.flush(&mut segments, scale_x, scale_y, page_height_pts);
                run.reset_bounds();
            }

            if !run.has_origin_y {
                run.origin_y = oy;
                run.has_origin_y = true;
            }
            run.left = run.left.min(ox);
            let char_w = ch
                .loose_bounds()
                .ok()
                .map(|b| {
                    #[allow(deprecated)]
                    {
                        b.right().value as f64 - b.left().value as f64
                    }
                })
                .unwrap_or(font_size_pts as f64 * 0.5);
            let cw = if char_w > 0.0 && char_w < font_size_pts as f64 * 2.0 {
                char_w
            } else {
                font_size_pts as f64 * 0.5
            };
            run.right = run.right.max(ox + cw);
        } else if let Ok(bounds) = ch.loose_bounds() {
            #[allow(deprecated)]
            {
                let l = bounds.left().value as f64;
                let r = bounds.right().value as f64;
                run.left = run.left.min(l);
                run.right = run.right.max(r);
                if !run.has_origin_y {
                    run.origin_y = bounds.bottom().value as f64;
                    run.has_origin_y = true;
                }
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
        segments: Arc::new(segments),
    })
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

/// Document-level metadata extracted from PDF properties (XMP / DocInfo).
#[derive(Debug, Clone, Default)]
pub struct PdfDocMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
}

/// Extract document-level metadata (title, author, subject) from PDF properties.
pub fn extract_doc_metadata(pdfium: &Pdfium, pdf_path: &str) -> Result<PdfDocMetadata, PdfError> {
    use pdfium_render::prelude::PdfDocumentMetadataTagType;

    let document = pdfium.load_pdf_from_file(pdf_path, None)?;
    let metadata = document.metadata();

    let get = |tag: PdfDocumentMetadataTagType| -> Option<String> {
        metadata
            .get(tag)
            .map(|t| t.value().to_string())
            .filter(|s| !s.trim().is_empty())
    };

    Ok(PdfDocMetadata {
        title: get(PdfDocumentMetadataTagType::Title),
        author: get(PdfDocumentMetadataTagType::Author),
        subject: get(PdfDocumentMetadataTagType::Subject),
    })
}

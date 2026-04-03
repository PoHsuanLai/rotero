use std::sync::Arc;

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
    if lower.contains("cmbx")
        || lower.contains("cmr")
        || lower.contains("cmmi")
        || lower.contains("cmsy")
        || lower.contains("cmex")
        || lower.contains("cmti")
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
/// Segments are wrapped in `Arc` so that cloning `PageTextData` (which happens
/// frequently during Dioxus render cycles) is cheap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTextData {
    pub page_index: u32,
    pub segments: Arc<Vec<TextSegment>>,
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
    let bytes = std::fs::read(pdf_path)
        .map_err(|e| PdfError::RenderError(format!("Failed to read {pdf_path}: {e}")))?;
    let document = pdfium.load_pdf_from_byte_vec(bytes, None)?;
    let page = document
        .pages()
        .get(page_index as u16)
        .map_err(|e| PdfError::RenderError(e.to_string()))?;

    let page_width_pts = page.width().value;
    let page_height_pts = page.height().value;

    // Scale factors from PDF points to actual image pixels
    let scale_x = img_width as f64 / page_width_pts as f64;
    let scale_y = img_height as f64 / page_height_pts as f64;

    let text = page
        .text()
        .map_err(|e| PdfError::RenderError(e.to_string()))?;

    // Character-level extraction: group consecutive chars by font into runs
    let all_chars = text.chars();

    let mut segments = Vec::new();

    // Current run state.
    // Uses ch.origin().y for baseline (with ascent subtracted for top — matching PDF.js).
    // Uses ch.loose_bounds() for x and width (more reliable for hyphenated words).
    struct Run {
        text: String,
        font_name: String,
        is_italic: bool,
        /// Left edge from loose_bounds of first char
        left: f64,
        /// Origin y (baseline) of first char (PDF points, page space)
        origin_y: f64,
        /// Right edge from loose_bounds of last char
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
            // Use loose_bounds for x and width (reliable for hyphenated words)
            let x = self.left * scale_x;
            let width = (self.right - self.left) * scale_x;
            // Use origin.y for baseline, with ascent subtracted (matches PDF.js)
            let y = if self.has_origin_y {
                let ascent_pts = self.font_size_pts as f64 * 0.8;
                let top_pts = self.origin_y + ascent_pts;
                (page_height_pts as f64 - top_pts) * scale_y
            } else {
                // Fallback: no origin available
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
            let font_weight = detect_font_weight(&self.font_name);
            let font_style = detect_font_style(&self.font_name, self.is_italic);

            // Sanity check: discard segments with unreasonably large bounds.
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

        // Flush on whitespace to produce per-word segments
        if c.is_whitespace() {
            run.flush(&mut segments, scale_x, scale_y, page_height_pts);
            run.reset_bounds();
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

        // Use origin() for positioning. Detect line wraps (hyphenation):
        // if char's origin.x jumps far left of the run's right edge, flush.
        if let Ok((ox, oy)) = ch.origin() {
            let ox = ox.value as f64;
            let oy = oy.value as f64;

            // Detect line wrap: char is far left of current run's right edge
            if !run.text.is_empty() && run.right > f64::MIN && ox < run.left - font_size_pts as f64 {
                run.flush(&mut segments, scale_x, scale_y, page_height_pts);
                run.reset_bounds();
            }

            if !run.has_origin_y {
                run.origin_y = oy;
                run.has_origin_y = true;
            }
            run.left = run.left.min(ox);
            // Track right edge: origin.x of this char + estimated char width
            // Use tight_bounds or loose_bounds width for the individual char
            let char_w = ch.loose_bounds().ok().map(|b| {
                #[allow(deprecated)]
                { b.right().value as f64 - b.left().value as f64 }
            }).unwrap_or(font_size_pts as f64 * 0.5);
            // Only use per-char bounds width if it's reasonable (not text-object-level)
            let cw = if char_w > 0.0 && char_w < font_size_pts as f64 * 2.0 {
                char_w
            } else {
                font_size_pts as f64 * 0.5
            };
            run.right = run.right.max(ox + cw);
        } else if let Ok(bounds) = ch.loose_bounds() {
            // Fallback when origin() not available
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
            let font_weight = detect_font_weight(&self.font_name);
            let font_style = detect_font_style(&self.font_name, self.is_italic);

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

            if !run.text.is_empty() && run.right > f64::MIN && ox < run.left - font_size_pts as f64 {
                run.flush(&mut segments, scale_x, scale_y, page_height_pts);
                run.reset_bounds();
            }

            if !run.has_origin_y {
                run.origin_y = oy;
                run.has_origin_y = true;
            }
            run.left = run.left.min(ox);
            let char_w = ch.loose_bounds().ok().map(|b| {
                #[allow(deprecated)]
                { b.right().value as f64 - b.left().value as f64 }
            }).unwrap_or(font_size_pts as f64 * 0.5);
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

/// A search match with its location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub page_index: u32,
    /// Bounding rectangles for the match (x, y, width, height in pixels).
    pub bounds: Vec<(f64, f64, f64, f64)>,
    pub matched_text: String,
}

/// Group segments into lines by y-proximity, sorted left-to-right within each line.
/// Returns indices into the original segments vec rather than references.
pub fn group_into_lines(segments: &[TextSegment]) -> Vec<Vec<usize>> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut indexed: Vec<usize> = (0..segments.len()).collect();
    indexed.sort_by(|&a, &b| {
        segments[a]
            .y
            .partial_cmp(&segments[b].y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current_line: Vec<usize> = vec![indexed[0]];
    let mut line_y = segments[indexed[0]].y;

    for &idx in &indexed[1..] {
        let seg = &segments[idx];
        let tolerance = seg.height * 0.5;
        if (seg.y - line_y).abs() < tolerance {
            current_line.push(idx);
        } else {
            current_line.sort_by(|&a, &b| {
                segments[a]
                    .x
                    .partial_cmp(&segments[b].x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            lines.push(current_line);
            current_line = vec![idx];
            line_y = seg.y;
        }
    }
    current_line.sort_by(|&a, &b| {
        segments[a]
            .x
            .partial_cmp(&segments[b].x)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lines.push(current_line);
    lines
}

/// Internal helper: group segments into lines returning references (used by search).
fn group_into_lines_ref(segments: &[TextSegment]) -> Vec<Vec<&TextSegment>> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut indexed: Vec<&TextSegment> = segments.iter().collect();
    indexed.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));

    let mut lines: Vec<Vec<&TextSegment>> = Vec::new();
    let mut current_line: Vec<&TextSegment> = vec![indexed[0]];
    let mut line_y = indexed[0].y;

    for seg in &indexed[1..] {
        let tolerance = seg.height * 0.5;
        if (seg.y - line_y).abs() < tolerance {
            current_line.push(seg);
        } else {
            current_line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
            lines.push(current_line);
            current_line = vec![seg];
            line_y = seg.y;
        }
    }
    current_line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    lines.push(current_line);
    lines
}

/// Search for text across all pages using already-extracted text data.
/// Concatenates same-line segments so multi-word queries match across word boundaries.
pub fn search_in_text_data(text_data: &[PageTextData], query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    for page_data in text_data {
        let lines = group_into_lines_ref(&page_data.segments);

        for line in &lines {
            // Build concatenated line text with space separators
            let mut concat = String::new();
            let mut seg_ranges: Vec<(usize, usize)> = Vec::new();

            for (i, seg) in line.iter().enumerate() {
                if i > 0 {
                    concat.push(' ');
                }
                let start = concat.len();
                concat.push_str(&seg.text);
                seg_ranges.push((start, concat.len()));
            }

            let concat_lower = concat.to_lowercase();
            let mut search_start = 0;
            while let Some(pos) = concat_lower[search_start..].find(&query_lower) {
                let abs_pos = search_start + pos;
                let match_end = abs_pos + query_lower.len();

                // Merge overlapping segment bounds into one continuous rect
                let mut min_x = f64::MAX;
                let mut min_y = f64::MAX;
                let mut max_right = f64::MIN;
                let mut max_bottom = f64::MIN;
                for (seg_idx, &(seg_start, seg_end)) in seg_ranges.iter().enumerate() {
                    if seg_end > abs_pos && seg_start < match_end {
                        let seg = &line[seg_idx];
                        min_x = min_x.min(seg.x);
                        min_y = min_y.min(seg.y);
                        max_right = max_right.max(seg.x + seg.width);
                        max_bottom = max_bottom.max(seg.y + seg.height);
                    }
                }
                let bounds = vec![(min_x, min_y, max_right - min_x, max_bottom - min_y)];

                matches.push(SearchMatch {
                    page_index: page_data.page_index,
                    bounds,
                    matched_text: concat[abs_pos..match_end].to_string(),
                });

                search_start = abs_pos + 1;
            }
        }
    }

    matches
}

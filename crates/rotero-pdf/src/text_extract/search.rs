//! Text search within PDF pages.
//!
//! Find UI search runs against `pdfrum::TextPage::find_with`. Line grouping and
//! `text_block_at` remain for citation helpers and segment-based search that still
//! work from extracted segments.

use pdfrum::{CharIndex, Document, FindOptions, Point, Rect, Size, TextIndex, TextPage};
use serde::{Deserialize, Serialize};

use super::{PageTextData, TextSegment};

/// A single search hit within the extracted text of a PDF page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    /// Zero-based page number containing this match.
    pub page_index: u32,
    /// Bounding rectangles (x, y, width, height in pixels).
    pub bounds: Vec<(f64, f64, f64, f64)>,
    /// The matched text as it appears in the document.
    pub matched_text: String,
}

/// Group segments into lines by y-proximity, sorted left-to-right within each line.
/// Returns indices into the original segments vec.
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

fn group_into_lines_ref(segments: &[TextSegment]) -> Vec<Vec<&TextSegment>> {
    group_into_lines(segments)
        .into_iter()
        .map(|idxs| idxs.into_iter().map(|i| &segments[i]).collect())
        .collect()
}

/// Extract a contiguous block of text starting at a vertical position on a page.
///
/// Used to pull a single reference-list entry out of a References section given
/// the target `y` (in the same pixel space as [`TextSegment::y`], origin
/// top-left) that an internal citation link points at. Groups `segments` into
/// lines, finds the first line at or below `start_y`, then accumulates lines
/// downward until a paragraph-sized vertical gap (the next reference) or
/// `max_lines` is reached. Returns the joined, trimmed text (lines separated by
/// `\n`), or an empty string if nothing sits at/below `start_y`.
///
/// `start_x` (the destination's left edge, when the link carries one) keeps
/// the block inside that column: on a two-column reference page each grouped
/// "line" otherwise splices both columns, mixing in the neighbouring entry's
/// title and DOI.
pub fn text_block_at(
    segments: &[TextSegment],
    start_x: Option<f64>,
    start_y: f64,
    max_lines: usize,
) -> String {
    let range = start_x.and_then(|x| column_range(segments, x, start_y));
    let segments: Vec<TextSegment> = segments
        .iter()
        .filter(|s| range.is_none_or(|(lo, hi)| s.x >= lo && s.x < hi))
        .map(reposition_wrapped_word)
        .collect();
    let lines = group_into_lines_ref(&segments);
    if lines.is_empty() {
        return String::new();
    }

    // Line y/height, using the tallest segment on the line as its metrics.
    let line_metrics = |line: &[&TextSegment]| -> (f64, f64) {
        let y = line.iter().map(|s| s.y).fold(f64::MAX, f64::min);
        let h = line.iter().map(|s| s.height).fold(0.0_f64, f64::max);
        (y, h)
    };
    let line_text = |line: &[&TextSegment]| -> String {
        line.iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    };

    // First line whose top is at/below start_y, allowing a small tolerance so a
    // link that lands mid-line still catches that line.
    let start = lines.iter().position(|line| {
        let (y, h) = line_metrics(line);
        y + h * 0.5 >= start_y
    });
    let Some(start) = start else {
        return String::new();
    };

    // Bibliographies set each entry with a hanging indent: first line at the
    // margin, continuation lines indented. When nearby lines show that layout,
    // a line back at the margin starts the next entry — the only boundary
    // author–year lists have (no `[n]`, and often no paragraph gap). A
    // one-line entry has no indented line of its own, so look at its
    // neighbours rather than just line 2.
    let left = |line: &[&TextSegment]| line.first().map_or(0.0, |s| s.x);
    let indented_after = |from: usize, margin: f64, tol: f64| {
        lines
            .iter()
            .skip(from + 1)
            .take(max_lines * 2)
            .any(|l| left(l) > margin + tol && left(l) < margin + tol * 12.0)
    };
    let mut start = start;
    if let (Some(a), Some(b)) = (lines.get(start), lines.get(start + 1)) {
        let tol = line_metrics(a).1.max(1.0) * 0.5;
        // Destination landed on the indented tail of the previous entry.
        if left(b) < left(a) - tol && indented_after(start + 1, left(b), tol) {
            start += 1;
        }
    }
    let first_left = left(&lines[start]);
    let tol = line_metrics(&lines[start]).1.max(1.0) * 0.5;
    let hanging = indented_after(start, first_left, tol);

    let mut out: Vec<String> = Vec::new();
    let (_, mut prev_h) = line_metrics(&lines[start]);
    let mut prev_bottom = {
        let (y, h) = line_metrics(&lines[start]);
        y + h
    };
    for (n, line) in lines[start..].iter().enumerate() {
        if out.len() >= max_lines {
            break;
        }
        let (y, h) = line_metrics(line);
        if !out.is_empty() {
            // A gap larger than ~1.8× line height marks a paragraph / the next
            // reference entry — stop before it.
            let gap = y - prev_bottom;
            if gap > prev_h.max(h) * 1.8 {
                break;
            }
        }
        if n > 0 && hanging && (left(line) - first_left).abs() <= tol {
            break;
        }
        let text = line_text(line);
        if !text.trim().is_empty() {
            out.push(text);
        }
        prev_bottom = y + h;
        prev_h = h;
    }

    out.join("\n").trim().to_string()
}

/// A word hyphenated across a line break ("convolu-" / "tions") comes back
/// from pdfium as one box spanning both lines: its top is the first line but
/// its left edge is the *next* line's start, so it sorts to the front of the
/// first line. Re-anchor it at its right edge (where "convolu-" really sits).
fn reposition_wrapped_word(s: &TextSegment) -> TextSegment {
    let mut s = s.clone();
    if is_wrapped_word(&s) {
        s.x += s.width;
        s.width = 0.0;
        s.height = s.font_size;
    }
    s
}

/// A soft-hyphenated word whose box spans more than one line.
fn is_wrapped_word(s: &TextSegment) -> bool {
    s.text.contains('\u{ad}') && s.font_size > 0.0 && s.height > s.font_size * 1.6
}

/// `[lo, hi)` x-range of the column containing `x`, or `None` for a
/// single-column page.
///
/// Looks for a vertical gutter in the middle of the page — a band no word
/// crosses — within the region the reference block will be read from.
fn column_range(segments: &[TextSegment], x: f64, start_y: f64) -> Option<(f64, f64)> {
    let mut heights: Vec<f64> = segments.iter().map(|s| s.height).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let line_h = heights.get(heights.len() / 2).copied()?.max(1.0);
    let band: Vec<&TextSegment> = segments
        .iter()
        .filter(|s| s.y + s.height >= start_y - line_h && s.y <= start_y + line_h * 12.0)
        // A word hyphenated across a line or column break comes back as one
        // box spanning both halves — it can bridge the gutter. Ignore those.
        .filter(|s| !is_wrapped_word(s))
        .collect();
    if band.len() < 4 {
        return None;
    }
    let left = band.iter().map(|s| s.x).fold(f64::MAX, f64::min);
    let right = band.iter().map(|s| s.x + s.width).fold(f64::MIN, f64::max);
    let width = right - left;
    if width <= 0.0 {
        return None;
    }
    let crosses = |g: f64| band.iter().any(|s| s.x <= g && s.x + s.width >= g);

    // Scan the middle 30–70% for runs of x no word touches. A gutter is about
    // two line-heights wide; the gap between words is a fraction of one, so
    // demand at least one line-height of clear space.
    let steps = 200;
    let step = width * 0.4 / steps as f64;
    let centre = left + width / 2.0;
    let mut best: Option<f64> = None;
    let mut run_start: Option<f64> = None;
    for i in 0..=steps + 1 {
        let g = left + width * 0.3 + step * i as f64;
        let clear = i <= steps && !crosses(g);
        match (clear, run_start) {
            (true, None) => run_start = Some(g),
            (false, Some(start)) => {
                let end = g - step;
                if end - start >= line_h {
                    let mid = (start + end) / 2.0;
                    if best.is_none_or(|b| (mid - centre).abs() < (b - centre).abs()) {
                        best = Some(mid);
                    }
                }
                run_start = None;
            }
            _ => {}
        }
    }
    let gutter = best?;
    // Both sides must actually hold text, or it's just a short line.
    let left_n = band.iter().filter(|s| s.x + s.width <= gutter).count();
    let right_n = band.iter().filter(|s| s.x >= gutter).count();
    if left_n < 2 || right_n < 2 {
        return None;
    }
    if x < gutter {
        Some((f64::MIN, gutter))
    } else {
        Some((gutter, f64::MAX))
    }
}

/// Case-insensitive options matching the previous segment-search behaviour.
fn default_find_options() -> FindOptions {
    FindOptions {
        match_case: false,
        match_whole_word: false,
        consecutive: false,
    }
}

/// Map a [`TextIndex`] hit range onto a half-open [`CharIndex`] range for
/// [`pdfrum::TextPage::rects`].
fn text_range_to_char_range(
    map: &pdfrum::IndexMap,
    range: &std::ops::Range<TextIndex>,
) -> Option<std::ops::Range<CharIndex>> {
    if range.start >= range.end {
        return None;
    }
    let start = map.char_index(range.start)?;
    // Exclusive end: last included text index maps to a char; rects want one past.
    let last = TextIndex::new(range.end.get().saturating_sub(1));
    let last_char = map.char_index(last)?;
    let end = CharIndex::new(last_char.get() + 1);
    Some(start..end)
}

/// Convert PDF page-space rects (origin bottom-left) into pixel-space
/// `(x, y, width, height)` boxes (origin top-left).
fn pdf_rects_to_pixel_bounds(
    rects: &[pdfrum::Rect],
    page_height_pts: f64,
    scale_x: f64,
    scale_y: f64,
) -> Vec<(f64, f64, f64, f64)> {
    rects
        .iter()
        .map(|r| {
            let r = r.abs();
            let x = r.x0 * scale_x;
            let y = (page_height_pts - r.y1) * scale_y;
            let width = r.width() * scale_x;
            let height = r.height().abs() * scale_y;
            (x, y, width, height)
        })
        .filter(|(_, _, w, h)| *w > 0.0 && *h > 0.0)
        .collect()
}

/// Search every page via [`pdfrum::TextPage::find_with`], mapping hits into
/// pixel-space [`SearchMatch`]es.
///
/// `page_pixel_dims[i]` is `(width_px, height_px)` for page `i` (the rendered
/// image size). Pages missing an entry are searched at 1 PDF point = 1 pixel.
///
/// Matching is case-insensitive, matching the previous Find UI behaviour.
pub fn search_in_document(
    doc: &Document,
    query: &str,
    page_pixel_dims: &[(u32, u32)],
) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let options = default_find_options();
    let mut matches = Vec::new();
    let page_count = doc.page_count();

    for page_index in 0..page_count {
        let Ok(page) = doc.page(page_index) else {
            continue;
        };
        let page_width_pts = page.width();
        let page_height_pts = page.height();
        let (img_w, img_h) = page_pixel_dims
            .get(page_index as usize)
            .copied()
            .unwrap_or((page_width_pts as u32, page_height_pts as u32));
        if img_w == 0 || img_h == 0 || page_width_pts <= 0.0 || page_height_pts <= 0.0 {
            continue;
        }
        let scale_x = f64::from(img_w) / page_width_pts;
        let scale_y = f64::from(img_h) / page_height_pts;

        let text = page.text();
        for range in text.find_with(query, options) {
            let matched_text: String = text.search_text[range.start.get()..range.end.get()]
                .iter()
                .collect();
            let Some(char_range) = text_range_to_char_range(&text.runs, &range) else {
                continue;
            };
            let pdf_rects = text.rects(char_range);
            let bounds = pdf_rects_to_pixel_bounds(&pdf_rects, page_height_pts, scale_x, scale_y);
            if bounds.is_empty() {
                continue;
            }
            matches.push(SearchMatch {
                page_index,
                bounds,
                matched_text,
            });
        }
    }

    matches
}

/// Segment-only search retained for callers that only have [`PageTextData`].
///
/// Prefer [`search_in_document`] for Find UI — it uses pdfrum's text index and
/// geometry. This path concatenates same-line segments so multi-word queries
/// still match across word boundaries when only extracted segments are available.
pub fn search_in_text_data(text_data: &[PageTextData], query: &str) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut matches = Vec::new();

    for page_data in text_data {
        let lines = group_into_lines_ref(&page_data.segments);

        for line in &lines {
            let mut concat = String::new();
            let mut seg_ranges: Vec<(usize, usize)> = Vec::new();

            for seg in line.iter() {
                let start = concat.len();
                concat.push_str(&seg.text);
                seg_ranges.push((start, concat.len()));
            }

            let concat_lower = concat.to_lowercase();
            let mut search_start = 0;
            while let Some(pos) = concat_lower[search_start..].find(&query_lower) {
                let abs_pos = search_start + pos;
                let match_end = abs_pos + query_lower.len();

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

/// Pixel-space result of intersecting a drag/selection rect with page text.
///
/// `line_rects` are Acrobat/Zotero-style: one axis-aligned box per text-object
/// run from [`TextPage::rects`] (top-left origin, same space as overlays /
/// [`write_annotations`](crate::write_annotations)).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionMarkup {
    /// One rect `(x, y, width, height)` per selected visual line / text object.
    pub line_rects: Vec<(f64, f64, f64, f64)>,
    /// Union of [`Self::line_rects`].
    pub bounds: (f64, f64, f64, f64),
    /// Selected text, lines joined by `\n`.
    pub text: String,
}

/// Build Highlight/Underline markup from a pixel-space drag rect via pdfrum
/// char-level APIs.
///
/// Approach: map the drag AABB into PDF page space (same scale / y-flip as
/// overlays and annotation writes), resolve a [`CharIndex`] range with
/// [`TextPage::index_at`] at the diagonally opposite corners (tolerance so a
/// drag that misses glyph centres still snaps), then take
/// [`TextPage::rects_loose`] for that range as the line quads (font em-box,
/// Acrobat-style markup geometry). If either corner misses, fall back to the
/// first/last character whose box intersects the selection. Returns [`None`]
/// when no characters are hit.
#[allow(clippy::too_many_arguments)] // page + pixel size + selection AABB
pub fn selection_markup(
    doc: &Document,
    page_index: u32,
    img_width: u32,
    img_height: u32,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
) -> Option<SelectionMarkup> {
    if sel_w <= 0.0 || sel_h <= 0.0 || img_width == 0 || img_height == 0 {
        return None;
    }
    let page = doc.page(page_index).ok()?;
    let page_width_pts = page.width();
    let page_height_pts = page.height();
    if page_width_pts <= 0.0 || page_height_pts <= 0.0 {
        return None;
    }
    let scale_x = f64::from(img_width) / page_width_pts;
    let scale_y = f64::from(img_height) / page_height_pts;
    let text = page.text();
    selection_markup_from_text(
        &text,
        page_height_pts,
        scale_x,
        scale_y,
        sel_x,
        sel_y,
        sel_w,
        sel_h,
    )
}

/// Same as [`selection_markup`] when the caller already holds a [`TextPage`].
#[allow(clippy::too_many_arguments)] // text page + scale + selection AABB
pub fn selection_markup_from_text(
    text: &TextPage,
    page_height_pts: f64,
    scale_x: f64,
    scale_y: f64,
    sel_x: f64,
    sel_y: f64,
    sel_w: f64,
    sel_h: f64,
) -> Option<SelectionMarkup> {
    if text.char_count() == 0 || sel_w <= 0.0 || sel_h <= 0.0 || scale_x <= 0.0 || scale_y <= 0.0 {
        return None;
    }

    // Pixel (top-left) → PDF (bottom-left), matching write_annotations / overlays.
    let pdf_x0 = sel_x / scale_x;
    let pdf_x1 = (sel_x + sel_w) / scale_x;
    let pdf_y1 = page_height_pts - (sel_y / scale_y); // top edge in PDF y-up
    let pdf_y0 = page_height_pts - ((sel_y + sel_h) / scale_y); // bottom edge
    let pdf_sel = Rect::new(
        pdf_x0.min(pdf_x1),
        pdf_y0.min(pdf_y1),
        pdf_x0.max(pdf_x1),
        pdf_y0.max(pdf_y1),
    );

    // Tolerance ~ half the shorter selection side (pts), floored so a thin drag
    // still snaps to nearby glyphs.
    let tol_w = (pdf_sel.width().abs() * 0.5).clamp(8.0, 36.0);
    let tol_h = (pdf_sel.height().abs() * 0.5).clamp(8.0, 36.0);
    let tolerance = Size::new(tol_w, tol_h);

    // Screen top-left / bottom-right → PDF points (y-up).
    let start = text.index_at(Point::new(pdf_sel.x0, pdf_sel.y1), tolerance);
    let end = text.index_at(Point::new(pdf_sel.x1, pdf_sel.y0), tolerance);

    let (from, to) = match (start, end) {
        (Some(a), Some(b)) => {
            let (lo, hi) = if a.get() <= b.get() { (a, b) } else { (b, a) };
            (lo, CharIndex::new(hi.get().saturating_add(1)))
        }
        _ => char_range_intersecting(text, pdf_sel)?,
    };

    markup_for_range(text, from, to, page_height_pts, scale_x, scale_y)
}

/// Line rects, bounds, and text for the character range `from..to`.
fn markup_for_range(
    text: &TextPage,
    from: CharIndex,
    to: CharIndex,
    page_height_pts: f64,
    scale_x: f64,
    scale_y: f64,
) -> Option<SelectionMarkup> {
    if from.get() >= to.get() {
        return None;
    }

    let pdf_rects = text.rects_loose(from..to);
    let line_rects = pdf_rects_to_pixel_bounds(&pdf_rects, page_height_pts, scale_x, scale_y);
    if line_rects.is_empty() {
        return None;
    }

    let mut bx0 = f64::MAX;
    let mut by0 = f64::MAX;
    let mut bx1 = f64::MIN;
    let mut by1 = f64::MIN;
    for &(x, y, w, h) in &line_rects {
        bx0 = bx0.min(x);
        by0 = by0.min(y);
        bx1 = bx1.max(x + w);
        by1 = by1.max(y + h);
    }

    let raw = text.slice(from..to);
    let text_out = raw.replace("\r\n", "\n").replace("\r", "\n");

    Some(SelectionMarkup {
        line_rects,
        bounds: (bx0, by0, bx1 - bx0, by1 - by0),
        text: text_out,
    })
}

/// Text selection for a drag from `anchor` (where the press happened) to
/// `focus` (where the pointer is now), both in pixel space.
///
/// Unlike [`selection_markup`], which reads the ends off a rectangle's
/// top-left and bottom-right corners, this resolves the character under each
/// point on its own, so the anchor end stays pinned wherever the pointer goes
/// — dragging up, left, or back across the start included.
pub fn selection_markup_between(
    doc: &Document,
    page_index: u32,
    img_width: u32,
    img_height: u32,
    anchor: (f64, f64),
    focus: (f64, f64),
) -> Option<SelectionMarkup> {
    if img_width == 0 || img_height == 0 {
        return None;
    }
    let page = doc.page(page_index).ok()?;
    let page_width_pts = page.width();
    let page_height_pts = page.height();
    if page_width_pts <= 0.0 || page_height_pts <= 0.0 {
        return None;
    }
    let text = page.text();
    selection_markup_between_from_text(
        &text,
        page_height_pts,
        f64::from(img_width) / page_width_pts,
        f64::from(img_height) / page_height_pts,
        anchor,
        focus,
    )
}

/// Same as [`selection_markup_between`] when the caller already holds a
/// [`TextPage`].
pub fn selection_markup_between_from_text(
    text: &TextPage,
    page_height_pts: f64,
    scale_x: f64,
    scale_y: f64,
    anchor: (f64, f64),
    focus: (f64, f64),
) -> Option<SelectionMarkup> {
    if text.char_count() == 0 || scale_x <= 0.0 || scale_y <= 0.0 {
        return None;
    }
    // Pixel (top-left origin) → PDF points (bottom-left origin).
    let to_pdf = |(x, y): (f64, f64)| Point::new(x / scale_x, page_height_pts - y / scale_y);
    // The glyph under the point, else the nearest glyph anywhere. Each end is
    // resolved from its own point only — never from a rectangle spanning both —
    // so the anchor stays put even when the pointer is in a margin or gutter.
    let resolve = |p: Point| {
        text.index_at(p, Size::new(4.0, 4.0))
            .or_else(|| nearest_char(text, p))
    };

    let a = resolve(to_pdf(anchor))?;
    let f = resolve(to_pdf(focus))?;
    let (lo, hi) = if a.get() <= f.get() { (a, f) } else { (f, a) };
    markup_for_range(
        text,
        lo,
        CharIndex::new(hi.get().saturating_add(1)),
        page_height_pts,
        scale_x,
        scale_y,
    )
}

/// The character whose box is closest to `p` (PDF points), ignoring
/// zero-size boxes such as generated line breaks.
fn nearest_char(text: &TextPage, p: Point) -> Option<CharIndex> {
    text.chars
        .iter()
        .enumerate()
        .filter_map(|(i, info)| {
            let b = info.char_box;
            let (x0, x1, y0, y1) = (
                b.x0.min(b.x1),
                b.x0.max(b.x1),
                b.y0.min(b.y1),
                b.y0.max(b.y1),
            );
            if x1 - x0 <= 0.0 || y1 - y0 <= 0.0 {
                return None;
            }
            let dx = (x0 - p.x).max(p.x - x1).max(0.0);
            let dy = (y0 - p.y).max(p.y - y1).max(0.0);
            Some((i, dx * dx + dy * dy))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| CharIndex::new(i))
}

/// How a click expands into a selection when there is no drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickSelectMode {
    /// The word under the point ([`TextPage::words`]).
    Word,
    /// The visual line under the point: all characters sharing the hit
    /// character's text object (same run as [`TextPage::rects`] grouping).
    Line,
}

/// Select the word or line under a pixel-space click via pdfrum word / char bounds.
///
/// Returns [`None`] when the point misses every character (within a modest
/// snap tolerance) or the page has no text.
pub fn selection_at_point(
    doc: &Document,
    page_index: u32,
    img_width: u32,
    img_height: u32,
    pixel_x: f64,
    pixel_y: f64,
    mode: ClickSelectMode,
) -> Option<SelectionMarkup> {
    if img_width == 0 || img_height == 0 {
        return None;
    }
    let page = doc.page(page_index).ok()?;
    let page_width_pts = page.width();
    let page_height_pts = page.height();
    if page_width_pts <= 0.0 || page_height_pts <= 0.0 {
        return None;
    }
    let scale_x = f64::from(img_width) / page_width_pts;
    let scale_y = f64::from(img_height) / page_height_pts;
    let text = page.text();
    selection_at_point_from_text(
        &text,
        page_height_pts,
        scale_x,
        scale_y,
        pixel_x,
        pixel_y,
        mode,
    )
}

/// Same as [`selection_at_point`] when the caller already holds a [`TextPage`].
pub fn selection_at_point_from_text(
    text: &TextPage,
    page_height_pts: f64,
    scale_x: f64,
    scale_y: f64,
    pixel_x: f64,
    pixel_y: f64,
    mode: ClickSelectMode,
) -> Option<SelectionMarkup> {
    if text.char_count() == 0 || scale_x <= 0.0 || scale_y <= 0.0 {
        return None;
    }
    let pdf_x = pixel_x / scale_x;
    let pdf_y = page_height_pts - (pixel_y / scale_y);
    // ~ half a typical glyph so a click near a word still snaps.
    let tolerance = Size::new(12.0, 12.0);
    let hit = text.index_at(Point::new(pdf_x, pdf_y), tolerance)?;

    let (from, to) = match mode {
        ClickSelectMode::Word => {
            let words = text.words();
            let word = words.into_iter().find(|w| {
                let start = w.range.start.get();
                let end = w.range.end.get();
                hit.get() >= start && hit.get() < end
            })?;
            (word.range.start, word.range.end)
        }
        ClickSelectMode::Line => {
            let obj = text.chars.get(hit.get())?.object?;
            let mut first = None;
            let mut last = None;
            for (i, info) in text.chars.iter().enumerate() {
                if info.object != Some(obj) {
                    continue;
                }
                if first.is_none() {
                    first = Some(i);
                }
                last = Some(i);
            }
            let (f, l) = (first?, last?);
            (CharIndex::new(f), CharIndex::new(l.saturating_add(1)))
        }
    };

    if from.get() >= to.get() {
        return None;
    }
    markup_from_char_range(text, page_height_pts, scale_x, scale_y, from, to)
}

fn markup_from_char_range(
    text: &TextPage,
    page_height_pts: f64,
    scale_x: f64,
    scale_y: f64,
    from: CharIndex,
    to: CharIndex,
) -> Option<SelectionMarkup> {
    let pdf_rects = text.rects_loose(from..to);
    let line_rects = pdf_rects_to_pixel_bounds(&pdf_rects, page_height_pts, scale_x, scale_y);
    if line_rects.is_empty() {
        return None;
    }
    let mut bx0 = f64::MAX;
    let mut by0 = f64::MAX;
    let mut bx1 = f64::MIN;
    let mut by1 = f64::MIN;
    for &(x, y, w, h) in &line_rects {
        bx0 = bx0.min(x);
        by0 = by0.min(y);
        bx1 = bx1.max(x + w);
        by1 = by1.max(y + h);
    }
    let raw = text.slice(from..to);
    let text_out = raw.replace("\r\n", "\n").replace("\r", "\n");
    Some(SelectionMarkup {
        line_rects,
        bounds: (bx0, by0, bx1 - bx0, by1 - by0),
        text: text_out,
    })
}

/// First..=last character whose glyph box intersects `sel` (inclusive end+1).
fn char_range_intersecting(text: &TextPage, sel: Rect) -> Option<(CharIndex, CharIndex)> {
    let mut first = None;
    let mut last = None;
    for (i, info) in text.chars.iter().enumerate() {
        let b = info.char_box;
        let bx0 = b.x0.min(b.x1);
        let by0 = b.y0.min(b.y1);
        let bx1 = b.x0.max(b.x1);
        let by1 = b.y0.max(b.y1);
        if bx1 < sel.x0 || bx0 > sel.x1 || by1 < sel.y0 || by0 > sel.y1 {
            continue;
        }
        if first.is_none() {
            first = Some(i);
        }
        last = Some(i);
    }
    let (f, l) = (first?, last?);
    Some((CharIndex::new(f), CharIndex::new(l.saturating_add(1))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn text_block_at_stays_in_destination_column() {
        // Two columns of references, gutter between x=60 and x=100.
        let segs = vec![
            seg("[1]", 0.0, 100.0),
            seg("Left", 30.0, 100.0),
            seg("[7]", 100.0, 100.0),
            seg("Right", 130.0, 100.0),
            seg("title", 0.0, 110.0),
            seg("one", 30.0, 110.0),
            seg("arXiv:1207.0580", 100.0, 110.0),
            seg("two", 130.0, 110.0),
        ];
        let left = text_block_at(&segs, Some(0.0), 100.0, 10);
        assert_eq!(left, "[1] Left\ntitle one");
        let right = text_block_at(&segs, Some(100.0), 100.0, 10);
        assert_eq!(right, "[7] Right\narXiv:1207.0580 two");
        // No x: old full-width behaviour.
        assert!(text_block_at(&segs, None, 100.0, 10).contains("Right"));
    }

    #[test]
    fn column_split_ignores_word_wrapped_across_columns() {
        // Latent Diffusion p.12: "Con\u{ad}ference" broken from the bottom of
        // the left column to the top of the right gets one page-tall box that
        // bridges the gutter.
        let mut bridge = seg("Con\u{ad}ference", 50.0, 90.0);
        bridge.width = 100.0;
        bridge.height = 600.0;
        let segs = vec![
            bridge,
            seg("[88]", 0.0, 100.0),
            seg("Suvorov", 30.0, 100.0),
            seg("[101]", 100.0, 100.0),
            seg("Yan", 130.0, 100.0),
            seg("inpainting", 0.0, 110.0),
            seg("VideoGPT", 130.0, 110.0),
        ];
        let right = text_block_at(&segs, Some(100.0), 100.0, 10);
        assert!(!right.contains("Suvorov"), "block was: {right:?}");
        assert!(right.contains("VideoGPT"), "block was: {right:?}");
    }

    #[test]
    fn text_block_at_puts_wrapped_word_at_line_end() {
        // Shape taken from the ResNet PDF: "convolu\u{ad}tions." is one box
        // spanning two lines, left edge at the second line's start.
        let mut wrapped = seg("convolu\u{ad}tions.", 0.0, 100.0);
        wrapped.width = 120.0;
        wrapped.height = 20.0;
        let segs = vec![
            seg("[44]", 0.0, 100.0),
            seg("Going", 30.0, 100.0),
            seg("deeper", 60.0, 100.0),
            seg("with", 90.0, 100.0),
            wrapped,
            seg("In", 0.0, 110.0),
            seg("CVPR.", 30.0, 110.0),
        ];
        let block = text_block_at(&segs, None, 100.0, 10);
        assert_eq!(
            block,
            "[44] Going deeper with convolu\u{ad}tions.\nIn CVPR."
        );
    }

    #[test]
    fn text_block_at_stops_at_next_hanging_indent_entry() {
        // Author–year list (DDIM): no enumerators, no paragraph gap, but each
        // entry's continuation lines are indented.
        let segs = vec![
            seg("Tail", 10.0, 90.0),
            seg("Kingma.", 0.0, 100.0),
            seg("Auto-encoding", 30.0, 100.0),
            seg("arXiv:1312.6114", 10.0, 110.0),
            seg("Levy.", 0.0, 120.0),
            seg("Generalizing", 30.0, 120.0),
            seg("carlo", 10.0, 130.0),
        ];
        assert_eq!(
            text_block_at(&segs, None, 100.0, 10),
            "Kingma. Auto-encoding\narXiv:1312.6114"
        );
        // Landing on the previous entry's indented tail skips it.
        assert_eq!(
            text_block_at(&segs, None, 90.0, 10),
            "Kingma. Auto-encoding\narXiv:1312.6114"
        );
    }

    #[test]
    fn text_block_at_one_line_entry_stops_at_next() {
        // DDIM: Bishop's PRML entry fits one line; the next entry follows flush.
        let segs = vec![
            seg("Bishop.", 0.0, 100.0),
            seg("PRML.", 30.0, 100.0),
            seg("Brock.", 0.0, 110.0),
            seg("Large", 30.0, 110.0),
            seg("scale", 10.0, 120.0),
        ];
        assert_eq!(text_block_at(&segs, None, 100.0, 10), "Bishop. PRML.");
    }

    #[test]
    fn text_block_at_without_hanging_indent_reads_on() {
        // Flush-left continuation lines: no indent signal, keep reading.
        let segs = vec![
            seg("Kingma.", 0.0, 100.0),
            seg("Auto-encoding", 0.0, 110.0),
            seg("bayes.", 0.0, 120.0),
        ];
        assert_eq!(
            text_block_at(&segs, None, 100.0, 10),
            "Kingma.\nAuto-encoding\nbayes."
        );
    }

    #[test]
    fn text_block_at_single_column_is_not_split() {
        // Words tile the full width with no gutter.
        let segs: Vec<_> = (0..8)
            .flat_map(|i| {
                let x = i as f64 * 20.0;
                [seg("w", x, 100.0), seg("w", x, 110.0)]
            })
            .collect();
        let block = text_block_at(&segs, Some(0.0), 100.0, 10);
        assert_eq!(block.split_whitespace().count(), 16);
    }

    /// One word-segment at (x, y) with a fixed height/width; other font fields
    /// are irrelevant to line grouping.
    fn seg(text: &str, x: f64, y: f64) -> TextSegment {
        TextSegment {
            text: text.to_string(),
            x,
            y,
            width: 20.0,
            height: 10.0,
            font_size: 10.0,
            font_family: "serif".into(),
            font_weight: "normal".into(),
            font_style: "normal".into(),
        }
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    #[test]
    fn text_block_at_starts_at_y_and_stops_at_paragraph_gap() {
        // Two reference entries, each two lines, separated by a large gap.
        // ref A: y=100,110 ; ref B: y=140,150 (gap 100->... A bottom 120, B top 140 = 20 > 1.8*10)
        let segs = vec![
            seg("Ng", 0.0, 100.0),
            seg("A.", 25.0, 100.0),
            seg("Inverse", 0.0, 110.0),
            seg("RL", 25.0, 110.0),
            seg("Smith", 0.0, 140.0),
            seg("B.", 25.0, 140.0),
            seg("Deep", 0.0, 150.0),
        ];

        // Ask for the block starting at ref A.
        let block = text_block_at(&segs, None, 100.0, 10);
        assert!(block.starts_with("Ng"), "block was: {block:?}");
        assert!(block.contains("Inverse RL"), "block was: {block:?}");
        // Must stop before ref B's paragraph.
        assert!(
            !block.contains("Smith"),
            "block should stop at gap: {block:?}"
        );
    }

    #[test]
    fn text_block_at_respects_max_lines() {
        let segs = vec![
            seg("l1", 0.0, 10.0),
            seg("l2", 0.0, 20.0),
            seg("l3", 0.0, 30.0),
        ];
        let block = text_block_at(&segs, None, 0.0, 2);
        assert_eq!(block, "l1\nl2");
    }

    #[test]
    fn text_block_at_empty_when_nothing_below() {
        let segs = vec![seg("top", 0.0, 10.0)];
        assert_eq!(text_block_at(&segs, None, 500.0, 10), "");
        assert_eq!(text_block_at(&[], None, 0.0, 10), "");
    }

    #[test]
    fn search_in_document_finds_hits_with_pixel_geometry() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open fixture");
        // Use 1:1 pts→px so bounds sit in page space numerically.
        let dims: Vec<(u32, u32)> = (0..doc.page_count())
            .map(|i| {
                let page = doc.page(i).expect("page");
                (page.width() as u32, page.height() as u32)
            })
            .collect();

        let hits = search_in_document(&doc, "Chapter", &dims);
        assert!(!hits.is_empty(), "expected at least one hit for 'Chapter'");
        let first = &hits[0];
        assert_eq!(first.page_index, 0);
        assert!(
            first.matched_text.to_lowercase().contains("chapter"),
            "matched_text={:?}",
            first.matched_text
        );
        assert!(!first.bounds.is_empty());
        for &(x, y, w, h) in &first.bounds {
            assert!(w > 0.0 && h > 0.0, "degenerate bound ({x},{y},{w},{h})");
            assert!(x >= 0.0 && y >= 0.0, "negative origin ({x},{y})");
        }
    }

    #[test]
    fn search_in_document_is_case_insensitive() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open fixture");
        let dims = [(595u32, 842u32)];
        let lower = search_in_document(&doc, "paragraph", &dims);
        let upper = search_in_document(&doc, "PARAGRAPH", &dims);
        assert_eq!(lower.len(), upper.len());
        assert!(!lower.is_empty());
    }

    #[test]
    fn search_in_document_empty_query_returns_nothing() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open fixture");
        assert!(search_in_document(&doc, "", &[(100, 100)]).is_empty());
    }

    #[test]
    fn selection_markup_char_level_multi_line_quads() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open");
        let page = doc.page(0).expect("page");
        let pw = page.width();
        let ph = page.height();
        let img_w = pw as u32;
        let img_h = ph as u32;
        // 1:1 pts→px. Cover a tall band through the body text so multiple
        // text objects fall inside the drag.
        let m = selection_markup(&doc, 0, img_w, img_h, 70.0, 100.0, 400.0, 120.0)
            .expect("markup over body text");
        assert!(
            m.line_rects.len() >= 2,
            "expected multi-line quads, got {} ({:?})",
            m.line_rects.len(),
            m.line_rects
        );
        for &(x, y, w, h) in &m.line_rects {
            assert!(w > 0.0 && h > 0.0, "degenerate ({x},{y},{w},{h})");
            assert!(x >= 0.0 && y >= 0.0);
        }
        let (bx, by, bw, bh) = m.bounds;
        assert!(bw > 0.0 && bh > 0.0, "empty bounds");
        assert!(bx >= 0.0 && by >= 0.0);
        assert!(!m.text.trim().is_empty(), "expected selected text");
    }

    /// Pixel centre of character `i` on a 1:1 (pt = px) rendering.
    fn char_center(text: &TextPage, page_h: f64, i: usize) -> (f64, f64) {
        let b = text.chars[i].char_box;
        ((b.x0 + b.x1) / 2.0, page_h - (b.y0 + b.y1) / 2.0)
    }

    #[test]
    fn selection_between_pins_the_anchor() {
        let doc = Document::open(fixture("basicapi.pdf")).expect("open");
        let page = doc.page(0).expect("page");
        let (pw, ph) = (page.width(), page.height());
        let text = page.text();
        let n = text.char_count();
        assert!(n > 40, "fixture should have text");
        let sel = |a: (f64, f64), f: (f64, f64)| {
            selection_markup_between(&doc, 0, pw as u32, ph as u32, a, f)
                .expect("selection")
                .text
        };
        // A visible glyph near the middle, so "starts/ends with" is meaningful.
        let visible = |i: usize| {
            let c = text.slice(CharIndex::new(i)..CharIndex::new(i + 1));
            let b = text.chars[i].char_box;
            !c.trim().is_empty() && (b.x1 - b.x0).abs() > 0.0 && (b.y1 - b.y0).abs() > 0.0
        };
        let anchor_i = (n / 2..n - 20)
            .find(|&i| visible(i))
            .expect("visible glyph");
        let anchor = char_center(&text, ph, anchor_i);
        let anchor_char = text.slice(CharIndex::new(anchor_i)..CharIndex::new(anchor_i + 1));

        // Focus later in reading order: selection starts at the anchor.
        for f in [anchor_i + 5, anchor_i + 15] {
            let s = sel(anchor, char_center(&text, ph, f));
            assert!(
                s.starts_with(&anchor_char),
                "{s:?} should start at anchor {anchor_char:?}"
            );
        }
        // Focus earlier: selection ends at the anchor, whichever way the
        // pointer sits relative to it on the page (the old box-corner logic
        // moved the anchor end here).
        for f in [anchor_i - 5, anchor_i - 15] {
            let s = sel(anchor, char_center(&text, ph, f));
            assert!(
                s.ends_with(&anchor_char),
                "{s:?} should end at anchor {anchor_char:?}"
            );
        }
        // Focus far off in the margin (no glyph near it): the anchor still holds
        // instead of falling back to a rectangle.
        for margin in [
            (-200.0, -200.0),
            (pw + 200.0, ph + 200.0),
            (anchor.0, ph + 300.0),
        ] {
            let s = sel(anchor, margin);
            assert!(
                s.starts_with(&anchor_char) || s.ends_with(&anchor_char),
                "{s:?} lost the anchor {anchor_char:?} with focus at {margin:?}"
            );
        }
        // Anchor and focus are interchangeable as a range.
        let (a, f) = (anchor, char_center(&text, ph, anchor_i + 15));
        assert_eq!(sel(a, f), sel(f, a));
    }

    #[test]
    fn selection_markup_returns_none_without_overlap() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open");
        let page = doc.page(0).expect("page");
        assert!(
            selection_markup(
                &doc,
                0,
                page.width() as u32,
                page.height() as u32,
                10_000.0,
                10_000.0,
                20.0,
                20.0
            )
            .is_none()
        );
        assert!(selection_markup(&doc, 0, 100, 100, 0.0, 0.0, 0.0, 10.0).is_none());
    }

    #[test]
    fn selection_markup_single_line_loose_quad() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open");
        let page = doc.page(0).expect("page");
        let text = page.text();
        // Pick a short char run on the first text object and build a drag that
        // covers just those glyphs (PDF space → pixel at 1:1). Markup quads use
        // loose em-boxes, so expected geometry comes from rects_loose.
        assert!(text.char_count() > 8, "fixture should have text");
        let boxes = text.rects_loose(CharIndex::new(0)..CharIndex::new(5));
        assert!(!boxes.is_empty());
        let r = boxes[0].abs();
        let ph = page.height();
        let sel_x = r.x0;
        let sel_y = ph - r.y1;
        let sel_w = r.width().max(1.0);
        let sel_h = r.height().abs().max(1.0);
        let m = selection_markup(
            &doc,
            0,
            page.width() as u32,
            page.height() as u32,
            sel_x,
            sel_y,
            sel_w,
            sel_h,
        )
        .expect("single-line markup");
        assert_eq!(
            m.line_rects.len(),
            1,
            "expected one loose em-box quad: {:?}",
            m.line_rects
        );
        let (x, y, w, h) = m.line_rects[0];
        assert!((x - sel_x).abs() < 2.0, "x={x} sel_x={sel_x}");
        assert!((y - sel_y).abs() < 2.0, "y={y} sel_y={sel_y}");
        assert!(w > 0.0 && h > 0.0);
        // Loose markup height is at least the tight ink height for the same run.
        let tight = text.rects(CharIndex::new(0)..CharIndex::new(5));
        assert!(!tight.is_empty());
        assert!(
            h + 0.5 >= tight[0].abs().height().abs(),
            "loose h={h} should cover tight ink {}",
            tight[0].abs().height()
        );
    }

    #[test]
    fn selection_at_point_word_and_line() {
        let path = fixture("basicapi.pdf");
        let doc = Document::open(&path).expect("open");
        let page = doc.page(0).expect("page");
        let pw = page.width();
        let ph = page.height();
        let text = page.text();
        let words = text.words();
        assert!(!words.is_empty(), "fixture should have words");
        let word = &words[0];
        let mid = word.rect.center();
        // PDF y-up → pixel top-left at 1:1
        let px = mid.x;
        let py = ph - mid.y;
        let w = selection_at_point_from_text(&text, ph, 1.0, 1.0, px, py, ClickSelectMode::Word)
            .expect("word select");
        assert!(
            w.text.contains(word.text.trim()) || word.text.trim().contains(w.text.trim()),
            "word text={:?} selected={:?}",
            word.text,
            w.text
        );
        assert!(!w.line_rects.is_empty());

        let line = selection_at_point_from_text(&text, ph, 1.0, 1.0, px, py, ClickSelectMode::Line)
            .expect("line select");
        assert!(
            line.text.len() >= w.text.len(),
            "line should be at least the word: word={:?} line={:?}",
            w.text,
            line.text
        );
        let _ = (pw, doc);
    }
}

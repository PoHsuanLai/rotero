//! Text search within extracted text data.

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

/// Extract a contiguous block of text starting at a vertical position on a page.
///
/// Used to pull a single reference-list entry out of a References section given
/// the target `y` (in the same pixel space as [`TextSegment::y`], origin
/// top-left) that an internal citation link points at. Groups `segments` into
/// lines, finds the first line at or below `start_y`, then accumulates lines
/// downward until a paragraph-sized vertical gap (the next reference) or
/// `max_lines` is reached. Returns the joined, trimmed text (lines separated by
/// `\n`), or an empty string if nothing sits at/below `start_y`.
pub fn text_block_at(segments: &[TextSegment], start_y: f64, max_lines: usize) -> String {
    let lines = group_into_lines_ref(segments);
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

    let mut out: Vec<String> = Vec::new();
    let (_, mut prev_h) = line_metrics(&lines[start]);
    let mut prev_bottom = {
        let (y, h) = line_metrics(&lines[start]);
        y + h
    };
    for line in &lines[start..] {
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
        let text = line_text(line);
        if !text.trim().is_empty() {
            out.push(text);
        }
        prev_bottom = y + h;
        prev_h = h;
    }

    out.join("\n").trim().to_string()
}

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

#[cfg(test)]
mod tests {
    use super::*;

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
        let block = text_block_at(&segs, 100.0, 10);
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
        let block = text_block_at(&segs, 0.0, 2);
        assert_eq!(block, "l1\nl2");
    }

    #[test]
    fn text_block_at_empty_when_nothing_below() {
        let segs = vec![seg("top", 0.0, 10.0)];
        assert_eq!(text_block_at(&segs, 500.0, 10), "");
        assert_eq!(text_block_at(&[], 0.0, 10), "");
    }
}

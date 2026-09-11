//! Annotation writing to PDF files via pdfrum's typed `DocEdit` API.
//!
//! Converts in-app annotations (highlights, notes, areas, underlines, ink, free text)
//! into PDF annotation dictionaries and writes them into the document.

use std::path::Path;

use pdfrum::{AnnotSpec, Color, Document, Point, Quad, Rect, SaveOptions};
use rotero_models::{Annotation, AnnotationType};

use crate::PdfError;

struct AnnotationGeometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    page_width: f32,
    page_height: f32,
}

/// `page_dimensions` provides (width_pts, height_pts) per 0-indexed page,
/// as returned by `PdfEngine::get_page_dimensions()`.
pub fn write_annotations(
    input_path: &Path,
    output_path: &Path,
    annotations: &[Annotation],
    page_dimensions: &[(f32, f32)],
) -> Result<(), PdfError> {
    let doc = Document::open(input_path)
        .map_err(|e| PdfError::WriteError(format!("Failed to load PDF: {e}")))?;
    let page_count = doc.page_count();
    let mut edit = doc.edit();

    for ann in annotations {
        if ann.page < 0 {
            continue;
        }
        let page = ann.page as u32;
        if page >= page_count {
            continue;
        }
        let Some(&(pw_pts, ph_pts)) = page_dimensions.get(page as usize) else {
            continue;
        };
        let geom = match parse_geometry(&ann.geometry) {
            Ok(g) => g,
            Err(_) => continue,
        };
        let rect = pixel_rect_to_pdf_rect(&geom, pw_pts, ph_pts);
        let color = hex_to_color(&ann.color);
        let contents = ann.content.clone().filter(|s| !s.is_empty());

        let spec = match ann.ann_type {
            AnnotationType::Highlight => AnnotSpec::Highlight {
                rect,
                color,
                quads: vec![Quad::from_rect(rect)],
                contents,
            },
            AnnotationType::Note => AnnotSpec::Text {
                rect,
                color,
                contents,
            },
            AnnotationType::Area => AnnotSpec::Square {
                rect,
                color,
                contents,
            },
            AnnotationType::Underline => AnnotSpec::Underline {
                rect,
                color,
                quads: vec![Quad::from_rect(rect)],
                contents,
            },
            AnnotationType::Ink => AnnotSpec::Ink {
                rect,
                color,
                strokes: ink_strokes(&ann.geometry, &geom, pw_pts, ph_pts),
                contents: None,
            },
            AnnotationType::Text => AnnotSpec::FreeText {
                rect,
                color,
                contents: ann.content.clone().unwrap_or_default(),
                da: "0 0 0 rg /Helvetica 12 Tf".into(),
            },
        };

        edit.add_annotation(page, spec)
            .map_err(|e| PdfError::WriteError(format!("Failed to add annotation: {e}")))?;
    }

    edit.save(output_path, &SaveOptions::default())
        .map_err(|e| PdfError::WriteError(format!("Failed to save PDF: {e}")))?;

    Ok(())
}

fn parse_geometry(geom: &serde_json::Value) -> Result<AnnotationGeometry, PdfError> {
    let get_f32 = |key: &str| -> Result<f32, PdfError> {
        geom.get(key)
            .and_then(|v| v.as_f64())
            .map(|v| v as f32)
            .ok_or_else(|| PdfError::WriteError(format!("Missing geometry field: {key}")))
    };
    Ok(AnnotationGeometry {
        x: get_f32("x")?,
        y: get_f32("y")?,
        width: get_f32("width")?,
        height: get_f32("height")?,
        page_width: get_f32("page_width")?,
        page_height: get_f32("page_height")?,
    })
}

/// Convert a pixel-space rect (top-left origin) to a PDF-point rect (bottom-left).
fn pixel_rect_to_pdf_rect(
    geom: &AnnotationGeometry,
    page_width_pts: f32,
    page_height_pts: f32,
) -> Rect {
    let scale_x = page_width_pts / geom.page_width;
    let scale_y = page_height_pts / geom.page_height;

    let x0 = f64::from(geom.x * scale_x);
    let x1 = f64::from((geom.x + geom.width) * scale_x);
    let y1 = f64::from(page_height_pts - (geom.y * scale_y));
    let y0 = f64::from(page_height_pts - ((geom.y + geom.height) * scale_y));

    Rect::new(x0, y0, x1, y1)
}

/// Convert a pixel-space point (top-left origin) to PDF page space.
fn pixel_point_to_pdf(
    x: f32,
    y: f32,
    geom: &AnnotationGeometry,
    page_width_pts: f32,
    page_height_pts: f32,
) -> Point {
    let scale_x = page_width_pts / geom.page_width;
    let scale_y = page_height_pts / geom.page_height;
    Point::new(
        f64::from(x * scale_x),
        f64::from(page_height_pts - (y * scale_y)),
    )
}

/// `geometry.points` is an array of strokes; each stroke is a flat `[x, y, …]`
/// list in pixel space (same origin as the annotation rect).
fn ink_strokes(
    geom: &serde_json::Value,
    page_geom: &AnnotationGeometry,
    page_width_pts: f32,
    page_height_pts: f32,
) -> Vec<Vec<Point>> {
    let Some(strokes) = geom.get("points").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    strokes
        .iter()
        .filter_map(|stroke| {
            let pairs = stroke.as_array()?;
            let coords: Vec<f64> = pairs.iter().filter_map(|p| p.as_f64()).collect();
            let (chunks, _odd) = coords.as_chunks::<2>();
            if chunks.is_empty() {
                return None;
            }
            Some(
                chunks
                    .iter()
                    .map(|[x, y]| {
                        pixel_point_to_pdf(
                            *x as f32,
                            *y as f32,
                            page_geom,
                            page_width_pts,
                            page_height_pts,
                        )
                    })
                    .collect(),
            )
        })
        .collect()
}

/// Parse `#rrggbb` into a DeviceRGB colour, falling back to white.
///
/// Guarded on length *and* ASCII: an annotation colour that was short, empty,
/// or non-ASCII — all of which can arrive over sync — must degrade to a default
/// rather than panic while writing the PDF.
fn hex_to_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 || !hex.as_bytes()[..6].is_ascii() {
        return Color::WHITE;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    Color::from_rgb8(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use pdfrum::{Name, Object, Subtype};
    use rotero_models::Annotation;
    use serde_json::json;

    fn sample_geom() -> AnnotationGeometry {
        AnnotationGeometry {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            page_width: 100.0,
            page_height: 200.0,
        }
    }

    fn color_rgb(color: Color) -> [u8; 3] {
        let [r, g, b, _] = color.components;
        [
            (r * 255.0).round().clamp(0.0, 255.0) as u8,
            (g * 255.0).round().clamp(0.0, 255.0) as u8,
            (b * 255.0).round().clamp(0.0, 255.0) as u8,
        ]
    }

    #[test]
    fn hex_to_color_parses_hash_and_bare_rgb() {
        assert_eq!(color_rgb(hex_to_color("#ff8800")), [255, 136, 0]);
        assert_eq!(color_rgb(hex_to_color("00aa44")), [0, 170, 68]);
        assert_eq!(color_rgb(hex_to_color("#FF0000")), [255, 0, 0]);
    }

    #[test]
    fn hex_to_color_falls_back_to_white() {
        assert_eq!(hex_to_color(""), Color::WHITE);
        assert_eq!(hex_to_color("#fff"), Color::WHITE);
        assert_eq!(hex_to_color("#ÿÿÿÿÿÿ"), Color::WHITE);
    }

    #[test]
    fn pixel_rect_maps_top_left_pixels_to_bottom_left_pdf() {
        // 1:1 scale: page is 100×200 both in pixels and in PDF points.
        let rect = pixel_rect_to_pdf_rect(&sample_geom(), 100.0, 200.0);
        assert!((rect.x0 - 10.0).abs() < 1e-4);
        assert!((rect.x1 - 40.0).abs() < 1e-4);
        assert!((rect.y0 - 140.0).abs() < 1e-4); // 200 - (20+40)
        assert!((rect.y1 - 180.0).abs() < 1e-4); // 200 - 20
    }

    #[test]
    fn ink_strokes_convert_pixel_pairs_to_page_space() {
        let geom_json = json!({
            "x": 10.0, "y": 20.0, "width": 30.0, "height": 40.0,
            "page_width": 100.0, "page_height": 200.0,
            "points": [[10.0, 20.0, 40.0, 60.0]],
        });
        let geom = parse_geometry(&geom_json).unwrap();
        let strokes = ink_strokes(&geom_json, &geom, 100.0, 200.0);
        assert_eq!(strokes.len(), 1);
        assert_eq!(strokes[0].len(), 2);
        assert!((strokes[0][0].x - 10.0).abs() < 1e-4);
        assert!((strokes[0][0].y - 180.0).abs() < 1e-4);
        assert!((strokes[0][1].x - 40.0).abs() < 1e-4);
        assert!((strokes[0][1].y - 140.0).abs() < 1e-4);
    }

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    fn sample_annotation(
        page: i32,
        ann_type: AnnotationType,
        color: &str,
        content: Option<&str>,
        geometry: serde_json::Value,
    ) -> Annotation {
        let now = Utc::now();
        Annotation {
            id: None,
            paper_id: "p".into(),
            page,
            ann_type,
            color: color.into(),
            content: content.map(str::to_string),
            geometry,
            created_at: now,
            modified_at: now,
        }
    }

    #[test]
    fn write_annotations_round_trips_each_subtype() {
        let input = fixture("tracemonkey.pdf");
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("annotated.pdf");

        let geom = json!({
            "x": 20.0, "y": 30.0, "width": 80.0, "height": 24.0,
            "page_width": 612.0, "page_height": 792.0,
        });
        let ink_geom = json!({
            "x": 20.0, "y": 30.0, "width": 80.0, "height": 40.0,
            "page_width": 612.0, "page_height": 792.0,
            "points": [[20.0, 30.0, 60.0, 50.0, 100.0, 70.0]],
        });

        let annotations = vec![
            sample_annotation(
                0,
                AnnotationType::Highlight,
                "#ffff00",
                Some("hi"),
                geom.clone(),
            ),
            sample_annotation(
                0,
                AnnotationType::Note,
                "#ff8800",
                Some("note"),
                geom.clone(),
            ),
            sample_annotation(0, AnnotationType::Area, "#00aa44", None, geom.clone()),
            sample_annotation(0, AnnotationType::Underline, "#0066ff", None, geom.clone()),
            sample_annotation(0, AnnotationType::Ink, "#000000", None, ink_geom),
            sample_annotation(0, AnnotationType::Text, "#333333", Some("free text"), geom),
        ];

        // tracemonkey is US Letter-ish; use the engine's own page size.
        let engine = crate::PdfEngine::new();
        let dims = engine
            .get_page_dimensions(input.to_str().unwrap())
            .expect("dims");

        write_annotations(&input, &output, &annotations, &dims).expect("write");

        let extracted = engine
            .extract_annotations(output.to_str().unwrap())
            .expect("extract");
        assert_eq!(extracted.len(), 6);
        assert_eq!(extracted[0].ann_type, AnnotationType::Highlight);
        assert_eq!(extracted[0].color, "#ffff00");
        assert_eq!(extracted[0].content.as_deref(), Some("hi"));
        assert_eq!(extracted[1].ann_type, AnnotationType::Note);
        assert_eq!(extracted[1].content.as_deref(), Some("note"));
        assert_eq!(extracted[2].ann_type, AnnotationType::Area);
        assert_eq!(extracted[3].ann_type, AnnotationType::Underline);
        assert_eq!(extracted[4].ann_type, AnnotationType::Ink);
        assert_eq!(extracted[5].ann_type, AnnotationType::Text);
        assert_eq!(extracted[5].content.as_deref(), Some("free text"));

        let doc = Document::open(&output).expect("reopen");
        let page = doc.page(0).expect("page");
        let annots: Vec<_> = page.annotations().collect();
        assert_eq!(annots.len(), 6);

        let highlight = annots
            .iter()
            .find(|a| a.subtype() == Subtype::Highlight)
            .expect("highlight");
        assert_eq!(highlight.quad_points().len(), 1);
        assert!((highlight.rect().x0 - extracted[0].rect_pts[0] as f64).abs() < 0.5);

        let underline = annots
            .iter()
            .find(|a| a.subtype() == Subtype::Underline)
            .expect("underline");
        assert_eq!(underline.quad_points().len(), 1);

        let ink = annots
            .iter()
            .find(|a| a.subtype() == Subtype::Ink)
            .expect("ink");
        let Some(Object::Array(list)) = ink.dict().raw(&Name::from("InkList")) else {
            panic!("InkList missing");
        };
        assert_eq!(list.len(), 1);
        let Some(Object::Array(stroke)) = list.raw_at(0) else {
            panic!("stroke is not an array");
        };
        assert_eq!(stroke.len(), 6); // three points
    }

    #[test]
    fn write_annotations_preserves_existing_links() {
        let input = fixture("basicapi.pdf");
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("with-highlight.pdf");

        let engine = crate::PdfEngine::new();
        let before = engine
            .extract_links(input.to_str().unwrap())
            .expect("links before");
        assert!(!before.is_empty());

        let dims = engine
            .get_page_dimensions(input.to_str().unwrap())
            .expect("dims");
        let ann = sample_annotation(
            0,
            AnnotationType::Highlight,
            "#ffff00",
            None,
            json!({
                "x": 10.0, "y": 10.0, "width": 40.0, "height": 12.0,
                "page_width": dims[0].0, "page_height": dims[0].1,
            }),
        );
        write_annotations(&input, &output, &[ann], &dims).expect("write");

        let after = engine
            .extract_links(output.to_str().unwrap())
            .expect("links after");
        assert_eq!(after.len(), before.len());

        let extracted = engine
            .extract_annotations(output.to_str().unwrap())
            .expect("extract");
        assert!(
            extracted
                .iter()
                .any(|a| a.ann_type == AnnotationType::Highlight)
        );
    }

    #[test]
    fn write_annotations_skips_out_of_range_pages() {
        let input = fixture("tracemonkey.pdf");
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("skip.pdf");
        let engine = crate::PdfEngine::new();
        let dims = engine
            .get_page_dimensions(input.to_str().unwrap())
            .expect("dims");

        let ann = sample_annotation(
            999,
            AnnotationType::Note,
            "#ff0000",
            Some("gone"),
            json!({
                "x": 1.0, "y": 1.0, "width": 10.0, "height": 10.0,
                "page_width": 100.0, "page_height": 100.0,
            }),
        );
        write_annotations(&input, &output, &[ann], &dims).expect("write");
        let extracted = engine
            .extract_annotations(output.to_str().unwrap())
            .expect("extract");
        assert!(extracted.is_empty());
    }
}

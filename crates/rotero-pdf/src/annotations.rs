use std::collections::HashMap;
use std::path::Path;

use lopdf::{Dictionary, Document, Object, StringFormat};
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
    let mut doc = Document::load(input_path)
        .map_err(|e| PdfError::WriteError(format!("Failed to load PDF: {e}")))?;

    let mut by_page: HashMap<i32, Vec<&Annotation>> = HashMap::new();
    for ann in annotations {
        by_page.entry(ann.page).or_default().push(ann);
    }

    // Collect page object IDs up front (page_iter borrows doc)
    let page_ids: Vec<_> = doc.page_iter().collect();

    for (page_num, page_anns) in &by_page {
        let page_idx = *page_num as usize;
        if page_idx >= page_dimensions.len() || page_idx >= page_ids.len() {
            continue;
        }
        let (pw_pts, ph_pts) = page_dimensions[page_idx];
        let page_id = page_ids[page_idx];

        let mut ann_refs = Vec::new();

        for ann in page_anns {
            let geom = match parse_geometry(&ann.geometry) {
                Ok(g) => g,
                Err(_) => continue,
            };
            let rect = pixel_rect_to_pdf_rect(&geom, pw_pts, ph_pts);
            let ann_dict = build_annotation_dict(ann, &rect);
            let ann_id = doc.add_object(ann_dict);
            ann_refs.push(Object::Reference(ann_id));
        }

        if ann_refs.is_empty() {
            continue;
        }

        let page_obj = doc
            .get_object_mut(page_id)
            .map_err(|e| PdfError::WriteError(format!("Cannot get page object: {e}")))?;

        if let Object::Dictionary(dict) = page_obj {
            match dict.get(b"Annots") {
                Ok(Object::Reference(annots_ref)) => {
                    let annots_ref = *annots_ref;
                    if let Ok(Object::Array(arr)) = doc.get_object_mut(annots_ref) {
                        arr.extend(ann_refs);
                    }
                }
                Ok(Object::Array(_)) => {
                    if let Ok(Object::Array(arr)) = dict.get_mut(b"Annots") {
                        arr.extend(ann_refs);
                    }
                }
                _ => {
                    dict.set("Annots", Object::Array(ann_refs));
                }
            }
        }
    }

    doc.save(output_path)
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

/// Convert pixel-space rect to PDF-point-space rect [x1, y1, x2, y2].
/// PDF origin is bottom-left; pixel origin is top-left.
fn pixel_rect_to_pdf_rect(
    geom: &AnnotationGeometry,
    page_width_pts: f32,
    page_height_pts: f32,
) -> [f32; 4] {
    let scale_x = page_width_pts / geom.page_width;
    let scale_y = page_height_pts / geom.page_height;

    let x1 = geom.x * scale_x;
    let x2 = (geom.x + geom.width) * scale_x;
    let y2 = page_height_pts - (geom.y * scale_y);
    let y1 = page_height_pts - ((geom.y + geom.height) * scale_y);

    [x1, y1, x2, y2]
}

fn hex_to_rgb(hex: &str) -> [f32; 3] {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255) as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255) as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255) as f32 / 255.0;
    [r, g, b]
}

fn pdf_string(text: &str) -> Object {
    if text.is_ascii() {
        Object::String(text.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF]; // UTF-16BE BOM
        for c in text.encode_utf16() {
            bytes.extend_from_slice(&c.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Hexadecimal)
    }
}

fn build_annotation_dict(ann: &Annotation, rect: &[f32; 4]) -> Object {
    let [r, g, b] = hex_to_rgb(&ann.color);
    let rect_array = Object::Array(rect.iter().map(|&v| Object::Real(v)).collect());
    let color_array = Object::Array(vec![Object::Real(r), Object::Real(g), Object::Real(b)]);

    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"Annot".to_vec()));
    dict.set("Rect", rect_array.clone());
    dict.set("C", color_array);
    dict.set("F", Object::Integer(4)); // Print flag

    match ann.ann_type {
        AnnotationType::Highlight => {
            dict.set("Subtype", Object::Name(b"Highlight".to_vec()));
            let [x1, y1, x2, y2] = rect;
            let quad = Object::Array(vec![
                Object::Real(*x1),
                Object::Real(*y2), // top-left
                Object::Real(*x2),
                Object::Real(*y2), // top-right
                Object::Real(*x1),
                Object::Real(*y1), // bottom-left
                Object::Real(*x2),
                Object::Real(*y1), // bottom-right
            ]);
            dict.set("QuadPoints", quad);
            if let Some(ref content) = ann.content
                && !content.is_empty()
            {
                dict.set("Contents", pdf_string(content));
            }
        }
        AnnotationType::Note => {
            dict.set("Subtype", Object::Name(b"Text".to_vec()));
            dict.set("Name", Object::Name(b"Comment".to_vec()));
            dict.set("Open", Object::Boolean(false));
            if let Some(ref content) = ann.content
                && !content.is_empty()
            {
                dict.set("Contents", pdf_string(content));
            }
        }
        AnnotationType::Area => {
            dict.set("Subtype", Object::Name(b"Square".to_vec()));
            let mut bs = Dictionary::new();
            bs.set("Type", Object::Name(b"Border".to_vec()));
            bs.set("W", Object::Real(2.0));
            bs.set("S", Object::Name(b"S".to_vec()));
            dict.set("BS", Object::Dictionary(bs));
            if let Some(ref content) = ann.content
                && !content.is_empty()
            {
                dict.set("Contents", pdf_string(content));
            }
        }
        AnnotationType::Underline => {
            dict.set("Subtype", Object::Name(b"Underline".to_vec()));
            let [x1, y1, x2, y2] = rect;
            let quad = Object::Array(vec![
                Object::Real(*x1),
                Object::Real(*y2),
                Object::Real(*x2),
                Object::Real(*y2),
                Object::Real(*x1),
                Object::Real(*y1),
                Object::Real(*x2),
                Object::Real(*y1),
            ]);
            dict.set("QuadPoints", quad);
            if let Some(ref content) = ann.content
                && !content.is_empty()
            {
                dict.set("Contents", pdf_string(content));
            }
        }
        AnnotationType::Ink => {
            dict.set("Subtype", Object::Name(b"Ink".to_vec()));
            // InkList: array of strokes, each stroke is an array of [x, y] pairs
            if let Some(points) = ann.geometry.get("points").and_then(|v| v.as_array()) {
                let mut ink_list = Vec::new();
                for stroke in points {
                    if let Some(pairs) = stroke.as_array() {
                        let stroke_pts: Vec<Object> = pairs
                            .iter()
                            .filter_map(|p| p.as_f64().map(|v| Object::Real(v as f32)))
                            .collect();
                        ink_list.push(Object::Array(stroke_pts));
                    }
                }
                dict.set("InkList", Object::Array(ink_list));
            }
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(2.0));
            bs.set("S", Object::Name(b"S".to_vec()));
            dict.set("BS", Object::Dictionary(bs));
        }
        AnnotationType::Text => {
            dict.set("Subtype", Object::Name(b"FreeText".to_vec()));
            if let Some(ref content) = ann.content
                && !content.is_empty()
            {
                dict.set("Contents", pdf_string(content));
            }
            let da = "0 0 0 rg /Helvetica 12 Tf".to_string();
            dict.set("DA", pdf_string(&da));
        }
    }

    Object::Dictionary(dict)
}

use dioxus::prelude::*;

use super::AnnCtxState;
use crate::state::app_state::AnnotationContextInfo;
use rotero_models::{Annotation, AnnotationType};

/// Build the SVG path for an ink stroke, relative to the annotation's origin.
///
/// Pairs are taken with `chunks_exact(2)` rather than indexed in a step-by-2
/// loop. A length check alone does not establish an even count, because parsing
/// the JSON drops any element that is not a number — so an odd count read one
/// past the end and panicked. Annotation geometry arrives over sync and from PDF
/// extraction, neither of which validates it, and a panic in a render path
/// blanks the whole window with no error shown.
///
/// Returns an empty string when there are not two complete points to draw.
fn ink_path_data(points: &[serde_json::Value], x: f64, y: f64) -> String {
    let coords: Vec<f64> = points.iter().filter_map(|v| v.as_f64()).collect();
    let mut pairs = coords.chunks_exact(2);
    let Some(first) = pairs.next() else {
        return String::new();
    };

    let mut d = format!("M{},{}", first[0] - x, first[1] - y);
    for pair in pairs {
        d.push_str(&format!(" L{},{}", pair[0] - x, pair[1] - y));
    }
    d
}

pub(crate) fn render_annotation(ann: &Annotation, mut ann_ctx: AnnCtxState) -> Element {
    let x = ann
        .geometry
        .get("x")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let y = ann
        .geometry
        .get("y")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let w = ann
        .geometry
        .get("width")
        .and_then(|v| v.as_f64())
        .unwrap_or(24.0);
    let h = ann
        .geometry
        .get("height")
        .and_then(|v| v.as_f64())
        .unwrap_or(24.0);
    let color = ann.color.clone();
    let ann_id = ann.id.clone().unwrap_or_default();
    let ann_type = ann.ann_type;
    let page = ann.page;
    let content = ann.content.clone().unwrap_or_default();
    let color_for_ctx = color.clone();

    let on_context = {
        let ann_id = ann_id.clone();
        move |evt: Event<MouseData>| {
            evt.prevent_default();
            ann_ctx.set(Some(AnnotationContextInfo {
                annotation_id: ann_id.clone(),
                ann_type,
                page,
                color: color_for_ctx.clone(),
                content: content.clone(),
                x: evt.client_coordinates().x,
                y: evt.client_coordinates().y,
            }));
        }
    };

    match ann.ann_type {
        AnnotationType::Highlight => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; background: {color}; opacity: 0.35; pointer-events: auto; border-radius: 2px; z-index: 3;", oncontextmenu: on_context }
        },
        AnnotationType::Note => {
            let icon_bg = ann.color.clone();
            let title = ann.content.as_deref().unwrap_or("Empty note").to_string();
            rsx! {
                div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: 20px; height: 20px; background: {icon_bg}; border-radius: 4px; border: 1px solid rgba(0,0,0,0.2); cursor: pointer; display: flex; align-items: center; justify-content: center; font-size: 12px; pointer-events: auto; z-index: 3;", title: "{title}", oncontextmenu: on_context, "N" }
            }
        }
        AnnotationType::Area => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; border: 2px solid {color}; pointer-events: auto; z-index: 3;", oncontextmenu: on_context }
        },
        AnnotationType::Underline => rsx! {
            div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; border-bottom: 2px solid {color}; pointer-events: auto; z-index: 3;", oncontextmenu: on_context }
        },
        AnnotationType::Ink => {
            let points = ann
                .geometry
                .get("points")
                .and_then(|v| v.as_array())
                .and_then(|strokes| strokes.first())
                .and_then(|s| s.as_array());
            let path_d = points
                .map(|pts| ink_path_data(pts, x, y))
                .unwrap_or_default();
            rsx! {
                svg {
                    key: "ann-{ann_id}",
                    style: "position: absolute; left: {x}px; top: {y}px; width: {w}px; height: {h}px; pointer-events: auto; z-index: 3; overflow: visible;",
                    oncontextmenu: on_context,
                    path { d: "{path_d}", stroke: "{color}", stroke_width: "2", fill: "none", stroke_linecap: "round", stroke_linejoin: "round" }
                }
            }
        }
        AnnotationType::Text => {
            let text = ann.content.as_deref().unwrap_or("").to_string();
            rsx! {
                div { key: "ann-{ann_id}", style: "position: absolute; left: {x}px; top: {y}px; min-width: 40px; padding: 2px 4px; background: rgba(255,255,200,0.9); border: 1px solid {color}; font-size: 12px; pointer-events: auto; z-index: 3; white-space: pre-wrap; color: #333;", oncontextmenu: on_context, "{text}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ink_path_data;
    use serde_json::json;

    fn pts(values: serde_json::Value) -> Vec<serde_json::Value> {
        values.as_array().unwrap().clone()
    }

    /// The shape that used to panic: parsing drops a non-numeric element, so the
    /// surviving count is odd even though the raw array was long enough. The
    /// trailing unpaired coordinate is dropped rather than read past the end.
    #[test]
    fn an_odd_coordinate_count_does_not_panic() {
        let d = ink_path_data(&pts(json!([0.0, 0.0, 10.0, 10.0, 20.0])), 0.0, 0.0);
        assert_eq!(d, "M0,0 L10,10");

        // Same thing via a non-numeric element, which is how it arises in practice.
        let d = ink_path_data(&pts(json!([0.0, 0.0, 10.0, "x", 20.0, 20.0])), 0.0, 0.0);
        assert_eq!(d, "M0,0 L10,20");
    }

    /// Too few points to draw anything at all.
    #[test]
    fn a_short_or_empty_stroke_yields_no_path() {
        assert_eq!(ink_path_data(&pts(json!([])), 0.0, 0.0), "");
        assert_eq!(ink_path_data(&pts(json!([5.0])), 0.0, 0.0), "");
        assert_eq!(ink_path_data(&pts(json!(["a", "b"])), 0.0, 0.0), "");
    }

    /// Ordinary strokes still render, offset by the annotation's origin.
    #[test]
    fn a_stroke_is_drawn_relative_to_its_origin() {
        let d = ink_path_data(&pts(json!([15.0, 25.0, 35.0, 45.0])), 5.0, 5.0);
        assert_eq!(d, "M10,20 L30,40");
    }
}

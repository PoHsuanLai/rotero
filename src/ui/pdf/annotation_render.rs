use dioxus::prelude::*;

use super::AnnCtxState;
use crate::state::app_state::AnnotationContextInfo;
use rotero_models::{Annotation, AnnotationType};

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
            let path_d = if let Some(pts) = points {
                let coords: Vec<f64> = pts.iter().filter_map(|v| v.as_f64()).collect();
                if coords.len() >= 4 {
                    let mut d = format!("M{},{}", coords[0] - x, coords[1] - y);
                    for i in (2..coords.len()).step_by(2) {
                        d.push_str(&format!(" L{},{}", coords[i] - x, coords[i + 1] - y));
                    }
                    d
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
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

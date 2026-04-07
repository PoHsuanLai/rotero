use std::sync::Arc;

use dioxus::prelude::*;

use super::{hex_to_rgba, AnnCtxState};
use crate::state::app_state::{
    AnnotationContextInfo, AnnotationMode, PdfTabManager, TabId, ViewerToolState,
};
use rotero_db::Database;
use rotero_models::{Annotation, AnnotationType};

#[component]
pub(crate) fn PdfPageWithOverlay(
    page_index: u32,
    base64_data: Arc<String>,
    mime: &'static str,
    width: u32,
    height: u32,
    zoom: f32,
    render_zoom: f32,
    tab_id: TabId,
) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let tools = use_context::<Signal<ViewerToolState>>();
    let db = use_context::<Database>();
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let ann_ctx = use_context::<AnnCtxState>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

    let mgr = tabs.read();
    let tab = mgr.tab();
    let paper_id = tab.paper_id.clone().unwrap_or_default();
    let pdf_path_for_cache = tab.pdf_path.clone();
    let page_annotations: Vec<Annotation> = tab
        .annotations
        .iter()
        .filter(|a| a.page == page_index as i32)
        .cloned()
        .collect();
    let text_segments: Arc<Vec<rotero_pdf::TextSegment>> = tab
        .render
        .text_data
        .get(&page_index)
        .map(|td| td.segments.clone())
        .unwrap_or_default()
        .into();
    let search_bounds: Vec<(f64, f64, f64, f64)> = tab
        .search
        .matches
        .iter()
        .filter(|m| m.page_index == page_index)
        .flat_map(|m| m.bounds.iter().copied())
        .collect();
    drop(mgr);

    let selection_color = {
        let hex = &config.read().pdf.selection_color;
        hex_to_rgba(hex, 0.3)
    };

    let t = tools.read();
    let mode = t.annotation_mode;
    let color = t.annotation_color.clone();
    drop(t);

    let cursor = match mode {
        AnnotationMode::Highlight | AnnotationMode::Underline => "crosshair",
        AnnotationMode::Note => "cell",
        AnnotationMode::Ink => "crosshair",
        AnnotationMode::Text => "text",
        AnnotationMode::None => "default",
    };

    // Images are rendered at render_zoom (= zoom * dpr). We use CSS zoom on
    // the wrapper to scale to display size. Unlike transform: scale(), CSS zoom
    // affects layout — the element's flow size changes with the zoom factor,
    // preventing page overlap.
    let css_zoom = if render_zoom > 0.0 {
        zoom / render_zoom
    } else {
        1.0
    };

    rsx! {
        div {
            class: "pdf-page-wrapper",
            style: "cursor: {cursor}; zoom: {css_zoom};",

            {
                // Use file:// URL from disk cache if available (short string, instant diff).
                // Fall back to inline base64 data URL for pages not yet cached.
                let data_dir = config.read().effective_library_path();
                let src = crate::cache::page_file_url(&data_dir, &pdf_path_for_cache, page_index, mime)
                    .unwrap_or_else(|| format!("data:{mime};base64,{base64_data}"));
                rsx! {
                    img {
                        class: "pdf-page-img",
                        src: "{src}",
                        width: "{width}",
                        height: "{height}",
                        draggable: "false",
                    }
                }
            }

            div {
                class: "text-layer",
                id: "text-layer-{page_index}",
                style: "width: {width}px; height: {height}px; --selection-color: {selection_color};",
                onmounted: move |_| {
                    spawn(async move {
                        let js = format!(r#"
                            (function() {{
                                let layer = document.getElementById('text-layer-{page_index}');
                                if (!layer) return;

                                // --- Text scaling ---
                                let spans = layer.querySelectorAll('span[data-target-w]');
                                let canvas = document.createElement('canvas');
                                let ctx = canvas.getContext('2d');

                                // Detect browser minimum font size
                                let probe = document.createElement('div');
                                probe.style.fontSize = '1px';
                                probe.style.lineHeight = '1';
                                probe.style.position = 'absolute';
                                probe.style.opacity = '0';
                                probe.textContent = 'X';
                                document.body.appendChild(probe);
                                let minFs = probe.getBoundingClientRect().height;
                                probe.remove();

                                for (let span of spans) {{
                                    if (span.textContent.trimEnd().length <= 1) continue;

                                    let targetW = parseFloat(span.dataset.targetW);
                                    let fontSize = parseFloat(span.style.fontSize);
                                    let fontStyle = span.dataset.fontStyle || 'normal';
                                    let fontWeight = span.dataset.fontWeight || 'normal';
                                    let fontFamily = span.style.fontFamily || 'sans-serif';

                                    let scaledFontSize = fontSize;
                                    if (minFs > 1) {{
                                        scaledFontSize = fontSize * minFs;
                                        span.style.fontSize = scaledFontSize + 'px';
                                    }}

                                    ctx.font = fontStyle + ' ' + fontWeight + ' ' + scaledFontSize + 'px ' + fontFamily;
                                    let measured = ctx.measureText(span.textContent.trimEnd()).width;

                                    let transform = '';
                                    if (minFs > 1) {{
                                        transform = 'scale(' + (1 / minFs) + ')';
                                    }}
                                    if (measured > 0 && targetW > 0) {{
                                        let sx = targetW / measured;
                                        transform = 'scaleX(' + sx + ') ' + transform;
                                    }}
                                    if (transform) span.style.transform = transform;
                                }}

                                // Copy handler: normalize text
                                layer.addEventListener('copy', function(evt) {{
                                    let sel = document.getSelection();
                                    if (sel) {{
                                        let text = sel.toString().normalize('NFC').replace(/\0/g, '').trim();
                                        evt.clipboardData.setData('text/plain', text);
                                        evt.preventDefault();
                                    }}
                                }});

                                // Per-layer mousedown: activate selecting mode
                                layer.addEventListener('mousedown', function(e) {{
                                    if (e.button !== 0) return;
                                    this.classList.add('selecting');
                                    let eoc = this.querySelector('.endOfContent');
                                    if (eoc) this.appendChild(eoc);
                                }});

                                // Global selection handlers (install once)
                                if (!window.__roteroSelectionInit) {{
                                    window.__roteroSelectionInit = true;
                                    let prevRange = null;

                                    document.addEventListener('selectionchange', function() {{
                                        let sel = document.getSelection();
                                        if (!sel || sel.rangeCount === 0) {{
                                            document.querySelectorAll('.text-layer.selecting').forEach(function(tl) {{
                                                tl.classList.remove('selecting');
                                                let eoc = tl.querySelector('.endOfContent');
                                                if (eoc) {{ tl.appendChild(eoc); eoc.style.width = ''; eoc.style.height = ''; }}
                                            }});
                                            return;
                                        }}

                                        let range = sel.getRangeAt(0);

                                        // Detect which end is being modified (PDF.js approach)
                                        let modifyStart = prevRange &&
                                            (range.compareBoundaryPoints(Range.END_TO_END, prevRange) === 0 ||
                                             range.compareBoundaryPoints(Range.START_TO_END, prevRange) === 0);

                                        let anchor = modifyStart ? range.startContainer : range.endContainer;
                                        if (anchor.nodeType === Node.TEXT_NODE) {{
                                            anchor = anchor.parentNode;
                                        }}

                                        // Edge case: endOffset === 0 means cursor is at start of node
                                        if (!modifyStart && range.endOffset === 0) {{
                                            try {{
                                                while (!anchor.previousSibling) {{
                                                    anchor = anchor.parentNode;
                                                }}
                                                anchor = anchor.previousSibling;
                                                while (anchor.childNodes && anchor.childNodes.length) {{
                                                    anchor = anchor.lastChild;
                                                }}
                                                if (anchor.nodeType === Node.TEXT_NODE) {{
                                                    anchor = anchor.parentNode;
                                                }}
                                            }} catch(e) {{}}
                                        }}

                                        let textLayer = anchor && anchor.closest && anchor.closest('.text-layer');
                                        if (!textLayer || !textLayer.classList.contains('selecting')) {{
                                            prevRange = range.cloneRange();
                                            return;
                                        }}

                                        let eoc = textLayer.querySelector('.endOfContent');
                                        if (eoc && anchor.parentElement === textLayer) {{
                                            eoc.style.width = textLayer.style.width;
                                            eoc.style.height = textLayer.style.height;
                                            textLayer.insertBefore(
                                                eoc,
                                                modifyStart ? anchor : anchor.nextSibling
                                            );
                                        }}

                                        prevRange = range.cloneRange();
                                    }});

                                    function resetEndOfContent() {{
                                        document.querySelectorAll('.text-layer.selecting').forEach(function(tl) {{
                                            tl.classList.remove('selecting');
                                            let eoc = tl.querySelector('.endOfContent');
                                            if (eoc) {{ tl.appendChild(eoc); eoc.style.width = ''; eoc.style.height = ''; }}
                                        }});
                                        prevRange = null;
                                    }}

                                    document.addEventListener('pointerup', resetEndOfContent);
                                    window.addEventListener('blur', resetEndOfContent);
                                    document.addEventListener('keyup', function(e) {{
                                        if (!document.querySelector('.text-layer.selecting')) {{
                                            resetEndOfContent();
                                        }}
                                    }});
                                }}
                            }})()
                        "#);
                        let _ = document::eval(&js);
                    });
                },
                {
                    let lines = rotero_pdf::group_into_lines(&text_segments);
                    let w = width as f64;
                    let h = height as f64;
                    rsx! {
                        for (line_idx, line) in lines.iter().enumerate() {
                            if line_idx > 0 {
                                br { key: "br-{page_index}-{line_idx}" }
                            }
                            for &seg_idx in line.iter() {
                                {
                                    let seg = &text_segments[seg_idx];
                                    let left_pct = seg.x / w * 100.0;
                                    let top_pct = seg.y / h * 100.0;
                                    rsx! {
                                        span {
                                            key: "text-{page_index}-{seg_idx}",
                                            "data-target-w": "{seg.width}",
                                            "data-font-weight": "{seg.font_weight}",
                                            "data-font-style": "{seg.font_style}",
                                            style: "left: {left_pct:.4}%; top: {top_pct:.4}%; font-size: {seg.font_size}px; font-family: {seg.font_family}; font-weight: {seg.font_weight}; font-style: {seg.font_style};",
                                            "{seg.text}"
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "endOfContent" }
                    }
                }
            }

            for (si, (sx, sy, sw, sh)) in search_bounds.iter().enumerate() {
                div {
                    key: "search-{page_index}-{si}",
                    style: "position: absolute; left: {sx}px; top: {sy}px; width: {sw}px; height: {sh}px; background: rgba(255, 165, 0, 0.35); pointer-events: none; z-index: 2; border-radius: 2px;",
                }
            }

            for ann in page_annotations.iter() {
                {render_annotation(ann, ann_ctx)}
            }

            if mode != AnnotationMode::None {
                {
                    let mut drag_start = use_signal(|| None::<(f64, f64)>);
                    let mut drag_current = use_signal(|| None::<(f64, f64)>);
                    let mut ink_points = use_signal(Vec::<f64>::new);
                    let drag_rect = if mode == AnnotationMode::Highlight || mode == AnnotationMode::Underline {
                        if let (Some(start), Some(current)) = (drag_start(), drag_current()) {
                            let x = start.0.min(current.0);
                            let y = start.1.min(current.1);
                            let w = (start.0 - current.0).abs();
                            let h = (start.1 - current.1).abs();
                            if w > 2.0 || h > 2.0 { Some((x, y, w, h)) } else { None }
                        } else { None }
                    } else { None };

                    rsx! {
                        if let Some((rx, ry, rw, rh)) = drag_rect {
                            div {
                                style: "position: absolute; left: {rx}px; top: {ry}px; width: {rw}px; height: {rh}px; background: {color}; opacity: 0.3; pointer-events: none; z-index: 5; border-radius: 2px;",
                            }
                        }
                        div {
                            class: "annotation-click-overlay",
                            onmousedown: move |evt| {
                                if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                                if mode == AnnotationMode::Highlight || mode == AnnotationMode::Underline {
                                    let coords = evt.element_coordinates();
                                    drag_start.set(Some((coords.x, coords.y)));
                                    drag_current.set(Some((coords.x, coords.y)));
                                }
                                if mode == AnnotationMode::Ink {
                                    let coords = evt.element_coordinates();
                                    drag_start.set(Some((coords.x, coords.y)));
                                    ink_points.with_mut(|pts| {
                                        pts.clear();
                                        pts.push(coords.x);
                                        pts.push(coords.y);
                                    });
                                }
                            },
                            onmousemove: move |evt| {
                                if (mode == AnnotationMode::Highlight || mode == AnnotationMode::Underline) && drag_start().is_some() {
                                    let coords = evt.element_coordinates();
                                    drag_current.set(Some((coords.x, coords.y)));
                                }
                                if mode == AnnotationMode::Ink && drag_start().is_some() {
                                    let coords = evt.element_coordinates();
                                    ink_points.with_mut(|pts| {
                                        pts.push(coords.x);
                                        pts.push(coords.y);
                                    });
                                }
                            },
                            onmouseup: move |evt| {
                                if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                                let coords = evt.element_coordinates();
                                let x = coords.x;
                                let y = coords.y;
                                let (ann_type, geometry) = match mode {
                                    AnnotationMode::Highlight | AnnotationMode::Underline => {
                                        let at = if mode == AnnotationMode::Highlight { AnnotationType::Highlight } else { AnnotationType::Underline };
                                        if let Some(start) = drag_start() {
                                            let rx = start.0.min(x); let ry = start.1.min(y);
                                            let rw = (start.0 - x).abs(); let rh = (start.1 - y).abs();
                                            if rw < 5.0 && rh < 5.0 {
                                                drag_start.set(None); drag_current.set(None); return;
                                            }
                                            (at, serde_json::json!({
                                                "x": rx, "y": ry, "width": rw, "height": rh,
                                                "page_width": width, "page_height": height,
                                            }))
                                        } else { return; }
                                    }
                                    AnnotationMode::Note => {
                                        (AnnotationType::Note, serde_json::json!({
                                            "x": x, "y": y, "width": 24.0, "height": 24.0,
                                            "page_width": width, "page_height": height,
                                        }))
                                    }
                                    AnnotationMode::Ink => {
                                        let pts = ink_points.read().clone();
                                        ink_points.with_mut(|p| p.clear());
                                        if pts.len() < 4 {
                                            drag_start.set(None); return;
                                        }
                                        // Compute bounding box
                                        let mut min_x = f64::MAX; let mut min_y = f64::MAX;
                                        let mut max_x = f64::MIN; let mut max_y = f64::MIN;
                                        for i in (0..pts.len()).step_by(2) {
                                            let px = pts[i]; let py = pts[i + 1];
                                            if px < min_x { min_x = px; }
                                            if py < min_y { min_y = py; }
                                            if px > max_x { max_x = px; }
                                            if py > max_y { max_y = py; }
                                        }
                                        (AnnotationType::Ink, serde_json::json!({
                                            "x": min_x, "y": min_y,
                                            "width": max_x - min_x, "height": max_y - min_y,
                                            "page_width": width, "page_height": height,
                                            "points": [pts],
                                        }))
                                    }
                                    AnnotationMode::Text => {
                                        (AnnotationType::Text, serde_json::json!({
                                            "x": x, "y": y, "width": 150.0, "height": 20.0,
                                            "page_width": width, "page_height": height,
                                        }))
                                    }
                                    AnnotationMode::None => return,
                                };
                                drag_start.set(None); drag_current.set(None);
                                let now = chrono::Utc::now();
                                let ann = Annotation {
                                    id: None, paper_id: paper_id.clone(), page: page_index as i32, ann_type,
                                    color: color.clone(),
                                    content: if matches!(ann_type, AnnotationType::Note | AnnotationType::Text) { Some(String::new()) } else { None },
                                    geometry, created_at: now, modified_at: now,
                                };
                                let db = db.clone();
                                spawn(async move {
                                    if let Ok(id) = rotero_db::annotations::insert_annotation(db.conn(), &ann).await {
                                        let mut ann = ann;
                                        ann.id = Some(id);
                                        undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Create(ann.clone())));
                                        tabs.with_mut(|m| m.tab_mut().annotations.push(ann));
                                    }
                                });
                            },
                            onclick: move |_| {},
                        }
                    }
                }
            }
        }
    }
}

fn render_annotation(ann: &Annotation, mut ann_ctx: AnnCtxState) -> Element {
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
            // Build SVG path from stored points
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

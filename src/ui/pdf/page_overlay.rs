use std::sync::Arc;

use dioxus::prelude::*;

use super::super::components::context_menu::{ContextMenu, ContextMenuItem};
use super::annotation_render::render_annotation;
use super::{AnnCtxState, hex_to_rgba};
use crate::app::PdfDocs;
use crate::state::app_state::{
    AnnotationMode, PdfTabManager, PdfTextSelection, TabId, ViewerToolState,
};
use rotero_db::Database;
use rotero_models::{Annotation, AnnotationType};
use rotero_pdf::ClickSelectMode;

/// Build Highlight/Underline geometry from char-level [`SelectionMarkup`], or
/// fall back to the raw drag rect when no text was hit.
fn markup_geometry_from_selection(
    markup: Option<&rotero_pdf::SelectionMarkup>,
    rx: f64,
    ry: f64,
    rw: f64,
    rh: f64,
    page_width: u32,
    page_height: u32,
) -> (serde_json::Value, Option<String>) {
    if let Some(m) = markup {
        let (bx, by, bw, bh) = m.bounds;
        let rects: Vec<serde_json::Value> = m
            .line_rects
            .iter()
            .map(|(x, y, w, h)| serde_json::json!({ "x": x, "y": y, "width": w, "height": h }))
            .collect();
        let geometry = serde_json::json!({
            "x": bx,
            "y": by,
            "width": bw,
            "height": bh,
            "page_width": page_width,
            "page_height": page_height,
            "rects": rects,
        });
        let content = {
            let t = m.text.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };
        (geometry, content)
    } else {
        (
            serde_json::json!({
                "x": rx,
                "y": ry,
                "width": rw,
                "height": rh,
                "page_width": page_width,
                "page_height": page_height,
            }),
            None,
        )
    }
}

/// Match the former DOM copy handler: NFC + strip NULs + trim.
pub(crate) fn normalize_selection_clipboard(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfc()
        .collect::<String>()
        .replace('\0', "")
        .trim()
        .to_string()
}

/// Copy active PDF text selection to the system clipboard via arboard.
pub(crate) fn copy_pdf_text_selection(tools: &Signal<ViewerToolState>) -> bool {
    let Some(sel) = tools.read().text_selection.clone() else {
        return false;
    };
    let text = normalize_selection_clipboard(&sel.text);
    if text.is_empty() {
        return false;
    }
    if let Ok(mut clip) = arboard::Clipboard::new() {
        let _ = clip.set_text(text);
        true
    } else {
        false
    }
}

/// Consecutive primary clicks within this window count as double / triple.
const CLICK_STREAK_MS: u128 = 450;
/// Max pixel distance between streak clicks.
const CLICK_STREAK_PX: f64 = 8.0;

fn click_streak(prev: &mut Option<(std::time::Instant, f64, f64, u8)>, x: f64, y: f64) -> u8 {
    let now = std::time::Instant::now();
    let count = match *prev {
        Some((t, px, py, n))
            if now.duration_since(t).as_millis() <= CLICK_STREAK_MS
                && (x - px).abs() <= CLICK_STREAK_PX
                && (y - py).abs() <= CLICK_STREAK_PX =>
        {
            n.saturating_add(1).min(3)
        }
        _ => 1,
    };
    *prev = Some((now, x, y, count));
    count
}

#[component]
pub(crate) fn PdfPageWithOverlay(
    page_index: u32,
    /// `None` while the page is outside the resident render window — the slot
    /// renders as a sized placeholder so the scroll container keeps full height
    /// and this element (and its stable `id`) always exists. Keeping one node
    /// type per page slot avoids Dioxus keyed-diff breakage when a page swaps
    /// between placeholder and rendered.
    base64_data: Option<Arc<String>>,
    mime: &'static str,
    width: u32,
    height: u32,
    zoom: f32,
    render_zoom: f32,
    tab_id: TabId,
) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut tools = use_context::<Signal<ViewerToolState>>();
    let docs = use_context::<PdfDocs>().get();
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
    let search_bounds: Vec<(f64, f64, f64, f64)> = tab
        .search
        .matches
        .iter()
        .filter(|m| m.page_index == page_index)
        .flat_map(|m| m.bounds.iter().copied())
        .collect();
    let page_links: Vec<crate::state::app_state::PageLink> =
        tab.links.get(&page_index).cloned().unwrap_or_default();
    drop(mgr);

    // Open citation preview card: `Some((link dest, viewport x, viewport y))`.
    let mut card = use_signal(|| None::<(crate::state::app_state::LinkDest, f64, f64)>);

    // Drag state shared by native text select and annotation tools (hooks must
    // run unconditionally — previously these lived inside `mode != None`).
    let mut drag_start = use_signal(|| None::<(f64, f64)>);
    let mut drag_current = use_signal(|| None::<(f64, f64)>);
    let mut ink_points = use_signal(Vec::<f64>::new);
    // Double / triple-click streak (Instant + last pixel + count).
    let mut click_streak_state = use_signal(|| None::<(std::time::Instant, f64, f64, u8)>);
    // Skip the tiny-drag mouseup clear after a multi-click select.
    let mut multi_click_select = use_signal(|| false);
    // Right-click Copy menu: viewport (x, y) when open.
    let mut sel_ctx = use_signal(|| None::<(f64, f64)>);

    let selection_color = {
        let hex = &config.read().pdf.selection_color;
        hex_to_rgba(hex, 0.3)
    };

    let t = tools.read();
    let mode = t.annotation_mode;
    let color = t.annotation_color.clone();
    let active_sel = t.text_selection.clone();
    drop(t);

    let cursor = match mode {
        AnnotationMode::Highlight | AnnotationMode::Underline => "crosshair",
        AnnotationMode::Note => "cell",
        AnnotationMode::Ink => "crosshair",
        AnnotationMode::Text => "text",
        AnnotationMode::None => "text",
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

    // Not-yet-rendered slot: render a sized placeholder with the SAME element type,
    // wrapper class, and id as a rendered page so it reserves scroll height and the
    // node type never changes when the real page arrives.
    let Some(base64_data) = base64_data else {
        return rsx! {
            div {
                class: "pdf-page-wrapper pdf-page-placeholder",
                id: "pdf-page-{page_index}",
                style: "width: {width}px; height: {height}px; zoom: {css_zoom};",
            }
        };
    };

    // Live drag preview rects (text select or Highlight/Underline).
    let drag_preview_rects: Vec<(f64, f64, f64, f64)> = {
        let selecting_text = mode == AnnotationMode::None;
        let markup_mode = mode == AnnotationMode::Highlight
            || mode == AnnotationMode::Underline
            || selecting_text;
        if markup_mode {
            if let (Some(start), Some(current)) = (drag_start(), drag_current()) {
                let x = start.0.min(current.0);
                let y = start.1.min(current.1);
                let w = (start.0 - current.0).abs();
                let h = (start.1 - current.1).abs();
                if w > 2.0 || h > 2.0 {
                    if let Some(m) = docs.selection_markup(
                        &pdf_path_for_cache,
                        page_index,
                        width,
                        height,
                        x,
                        y,
                        w,
                        h,
                    ) {
                        m.line_rects
                    } else if selecting_text {
                        Vec::new()
                    } else {
                        vec![(x, y, w, h)]
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    };

    let preview_fill = if mode == AnnotationMode::None {
        selection_color.clone()
    } else {
        color.clone()
    };
    let preview_opacity = if mode == AnnotationMode::None {
        "1"
    } else {
        "0.3"
    };

    let active_rects: Vec<(f64, f64, f64, f64)> = match &active_sel {
        Some(sel) if sel.page_index == page_index && drag_start().is_none() => {
            sel.line_rects.clone()
        }
        _ => Vec::new(),
    };

    // Separate clones for mutually exclusive overlays (both closures type-checked).
    let docs_select = docs.clone();
    let docs_select_up = docs.clone();
    let docs_annot = docs.clone();
    let path_select = pdf_path_for_cache.clone();
    let path_select_up = pdf_path_for_cache.clone();
    let path_annot = pdf_path_for_cache.clone();

    rsx! {
        div {
            class: "pdf-page-wrapper",
            id: "pdf-page-{page_index}",
            style: "cursor: {cursor}; zoom: {css_zoom};",

            {
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

            for (si, (sx, sy, sw, sh)) in search_bounds.iter().enumerate() {
                div {
                    key: "search-{page_index}-{si}",
                    style: "position: absolute; left: {sx}px; top: {sy}px; width: {sw}px; height: {sh}px; background: rgba(255, 165, 0, 0.35); pointer-events: none; z-index: 2; border-radius: 2px;",
                }
            }

            // Clickable intra-document link hotspots (citations, figures, sections).
            // Positioned by scaling the stored fractional rect to the rendered image
            // pixel size (width/height), so they track the page at any zoom.
            for (li, link) in page_links.iter().enumerate() {
                {
                    use crate::state::app_state::LinkDest;
                    let lx = link.x_frac * width as f64;
                    let ly = link.y_frac * height as f64;
                    let lw = link.w_frac * width as f64;
                    let lh = link.h_frac * height as f64;
                    let dest = link.dest.clone();
                    let is_external = matches!(dest, LinkDest::External { .. });
                    let title = match &dest {
                        LinkDest::External { uri } => uri.clone(),
                        LinkDest::Internal { .. } => String::new(),
                    };
                    let class = if is_external {
                        "pdf-link-hotspot pdf-link-external"
                    } else {
                        "pdf-link-hotspot"
                    };
                    rsx! {
                        div {
                            key: "link-{page_index}-{li}",
                            class,
                            title: "{title}",
                            style: "left: {lx}px; top: {ly}px; width: {lw}px; height: {lh}px;",
                            onclick: move |evt: Event<MouseData>| {
                                evt.stop_propagation();
                                // Open a preview card anchored at the click rather
                                // than navigating away — the card offers Jump/Open/
                                // Import as appropriate.
                                let c = evt.client_coordinates();
                                card.set(Some((dest.clone(), c.x, c.y)));
                            },
                        }
                    }
                }
            }

            if let Some((dest, cx, cy)) = card() {
                super::CitationCard {
                    x: cx,
                    y: cy,
                    css_zoom,
                    link: dest,
                    tab_id,
                    on_close: move |_| card.set(None),
                }
            }

            for ann in page_annotations.iter() {
                {render_annotation(ann, ann_ctx)}
            }

            // Active selection (committed on mouseup) — below interactive links/annots.
            for (pi, (rx, ry, rw, rh)) in active_rects.iter().enumerate() {
                div {
                    key: "sel-active-{page_index}-{pi}",
                    style: "position: absolute; left: {rx}px; top: {ry}px; width: {rw}px; height: {rh}px; background: {selection_color}; pointer-events: none; z-index: 2; border-radius: 2px;",
                }
            }

            // Drag preview (text select or Highlight/Underline).
            for (pi, (rx, ry, rw, rh)) in drag_preview_rects.iter().enumerate() {
                div {
                    key: "drag-preview-{page_index}-{pi}",
                    style: "position: absolute; left: {rx}px; top: {ry}px; width: {rw}px; height: {rh}px; background: {preview_fill}; opacity: {preview_opacity}; pointer-events: none; z-index: 5; border-radius: 2px;",
                }
            }

            // Native text selection: overlay under links/annotations so those
            // stay clickable; drag uses pdfrum `selection_markup`. Double-click
            // selects a word, triple-click a line (via word/char bounds).
            // Multi-page drag selection is out of scope — starting a drag on
            // another page clears the prior selection (per-page overlays).
            if mode == AnnotationMode::None {
                div {
                    class: "text-select-overlay",
                    onmousedown: move |evt| {
                        let btn = evt.trigger_button();
                        if btn == Some(dioxus::html::input_data::MouseButton::Secondary) {
                            evt.prevent_default();
                            let c = evt.client_coordinates();
                            // Only offer Copy when there is an active selection
                            // on this page.
                            let has = tools.read().text_selection.as_ref()
                                .is_some_and(|s| s.page_index == page_index && !s.text.trim().is_empty());
                            if has {
                                sel_ctx.set(Some((c.x, c.y)));
                            }
                            return;
                        }
                        if btn != Some(dioxus::html::input_data::MouseButton::Primary) {
                            return;
                        }
                        sel_ctx.set(None);
                        let coords = evt.element_coordinates();
                        let mut streak = 1u8;
                        click_streak_state.with_mut(|prev| {
                            streak = click_streak(prev, coords.x, coords.y);
                        });
                        if streak >= 2 {
                            let mode = if streak >= 3 {
                                ClickSelectMode::Line
                            } else {
                                ClickSelectMode::Word
                            };
                            multi_click_select.set(true);
                            drag_start.set(None);
                            drag_current.set(None);
                            if let Some(m) = docs_select.selection_at_point(
                                &path_select,
                                page_index,
                                width,
                                height,
                                coords.x,
                                coords.y,
                                mode,
                            ) {
                                let text = m.text;
                                if text.trim().is_empty() {
                                    tools.with_mut(|t| t.text_selection = None);
                                } else {
                                    tools.with_mut(|t| {
                                        t.text_selection = Some(PdfTextSelection {
                                            page_index,
                                            line_rects: m.line_rects,
                                            text,
                                        });
                                    });
                                }
                            } else {
                                tools.with_mut(|t| t.text_selection = None);
                            }
                            return;
                        }
                        multi_click_select.set(false);
                        tools.with_mut(|t| t.text_selection = None);
                        drag_start.set(Some((coords.x, coords.y)));
                        drag_current.set(Some((coords.x, coords.y)));
                    },
                    onmousemove: move |evt| {
                        if drag_start().is_some() {
                            let coords = evt.element_coordinates();
                            drag_current.set(Some((coords.x, coords.y)));
                        }
                    },
                    onmouseup: move |evt| {
                        if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) {
                            return;
                        }
                        if multi_click_select() {
                            multi_click_select.set(false);
                            drag_start.set(None);
                            drag_current.set(None);
                            return;
                        }
                        let coords = evt.element_coordinates();
                        if let Some(start) = drag_start() {
                            let rx = start.0.min(coords.x);
                            let ry = start.1.min(coords.y);
                            let rw = (start.0 - coords.x).abs();
                            let rh = (start.1 - coords.y).abs();
                            drag_start.set(None);
                            drag_current.set(None);
                            if rw < 3.0 && rh < 3.0 {
                                return;
                            }
                            if let Some(m) = docs_select_up.selection_markup(
                                &path_select_up,
                                page_index,
                                width,
                                height,
                                rx,
                                ry,
                                rw,
                                rh,
                            ) {
                                let text = m.text;
                                if text.trim().is_empty() {
                                    tools.with_mut(|t| t.text_selection = None);
                                } else {
                                    tools.with_mut(|t| {
                                        t.text_selection = Some(PdfTextSelection {
                                            page_index,
                                            line_rects: m.line_rects,
                                            text,
                                        });
                                    });
                                }
                            } else {
                                tools.with_mut(|t| t.text_selection = None);
                            }
                        }
                    },
                    oncontextmenu: move |evt| {
                        evt.prevent_default();
                    },
                }
            }

            if let Some((cx, cy)) = sel_ctx() {
                ContextMenu {
                    x: cx,
                    y: cy,
                    on_close: move |_| sel_ctx.set(None),
                    ContextMenuItem {
                        label: "Copy".to_string(),
                        icon: Some("bi-clipboard".to_string()),
                        on_click: move |_| {
                            let _ = copy_pdf_text_selection(&tools);
                            sel_ctx.set(None);
                        },
                    }
                }
            }

            if mode != AnnotationMode::None {
                div {
                        class: "annotation-click-overlay",
                        onmousedown: move |evt| {
                            if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                            tools.with_mut(|t| t.text_selection = None);
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
                            let (ann_type, geometry, selected_content) = match mode {
                                AnnotationMode::Highlight | AnnotationMode::Underline => {
                                    let at = if mode == AnnotationMode::Highlight { AnnotationType::Highlight } else { AnnotationType::Underline };
                                    if let Some(start) = drag_start() {
                                        let rx = start.0.min(x); let ry = start.1.min(y);
                                        let rw = (start.0 - x).abs(); let rh = (start.1 - y).abs();
                                        if rw < 5.0 && rh < 5.0 {
                                            drag_start.set(None); drag_current.set(None); return;
                                        }
                                        let markup = docs_annot.selection_markup(
                                            &path_annot, page_index, width, height, rx, ry, rw, rh,
                                        );
                                        let (geometry, content) = markup_geometry_from_selection(
                                            markup.as_ref(), rx, ry, rw, rh, width, height,
                                        );
                                        (at, geometry, content)
                                    } else { return; }
                                }
                                AnnotationMode::Note => {
                                    (AnnotationType::Note, serde_json::json!({
                                        "x": x, "y": y, "width": 24.0, "height": 24.0,
                                        "page_width": width, "page_height": height,
                                    }), Some(String::new()))
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
                                    }), None)
                                }
                                AnnotationMode::Text => {
                                    (AnnotationType::Text, serde_json::json!({
                                        "x": x, "y": y, "width": 150.0, "height": 20.0,
                                        "page_width": width, "page_height": height,
                                    }), Some(String::new()))
                                }
                                AnnotationMode::None => return,
                            };
                            drag_start.set(None); drag_current.set(None);
                            let now = chrono::Utc::now();
                            let ann = Annotation {
                                id: None, paper_id: paper_id.clone(), page: page_index as i32, ann_type,
                                color: color.clone(),
                                content: selected_content,
                                geometry, created_at: now, modified_at: now,
                            };
                            let db = db.clone();
                            spawn(async move {
                                if let Ok(id) = db.insert_annotation(&ann).await {
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

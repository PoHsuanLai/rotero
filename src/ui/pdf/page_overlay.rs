use std::sync::Arc;

use dioxus::prelude::*;

use super::annotation_render::render_annotation;
use super::{
    AnnCtxState, CitationCardCtx, OpenCitationCard, PageSpace, SelCopyMenuCtx, hex_to_rgba,
};
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

#[derive(Clone, Copy)]
struct OverlayPage {
    index: u32,
    width: u32,
    height: u32,
    display_scale: f64,
}

fn apply_text_selection(
    tools: &mut Signal<ViewerToolState>,
    page_index: u32,
    markup: Option<rotero_pdf::SelectionMarkup>,
) {
    match markup {
        Some(m) if !m.text.trim().is_empty() => {
            tools.with_mut(|t| {
                t.text_selection = Some(PdfTextSelection {
                    page_index,
                    line_rects: m.line_rects,
                    text: m.text,
                });
            });
        }
        _ => tools.with_mut(|t| t.text_selection = None),
    }
}

fn compute_drag_preview_rects(
    docs: &crate::state::commands::PdfDocs,
    pdf_path: &str,
    page: OverlayPage,
    mode: AnnotationMode,
    start: Option<(f64, f64)>,
    current: Option<(f64, f64)>,
) -> Vec<(f64, f64, f64, f64)> {
    let selecting_text = mode == AnnotationMode::None;
    let markup_mode = selecting_text
        || matches!(
            mode,
            AnnotationMode::Highlight
                | AnnotationMode::Underline
                | AnnotationMode::StrikeOut
                | AnnotationMode::Squiggly
        );
    if !markup_mode {
        return Vec::new();
    }
    let (Some(start), Some(current)) = (start, current) else {
        return Vec::new();
    };
    let x = start.0.min(current.0);
    let y = start.1.min(current.1);
    let w = (start.0 - current.0).abs();
    let h = (start.1 - current.1).abs();
    if w * page.display_scale <= 2.0 && h * page.display_scale <= 2.0 {
        return Vec::new();
    }
    if let Some(m) =
        docs.selection_markup(pdf_path, page.index, page.width, page.height, x, y, w, h)
    {
        m.line_rects
    } else if selecting_text {
        Vec::new()
    } else {
        vec![(x, y, w, h)]
    }
}

fn markup_ann_type(mode: AnnotationMode) -> Option<AnnotationType> {
    match mode {
        AnnotationMode::Highlight => Some(AnnotationType::Highlight),
        AnnotationMode::Underline => Some(AnnotationType::Underline),
        AnnotationMode::StrikeOut => Some(AnnotationType::StrikeOut),
        AnnotationMode::Squiggly => Some(AnnotationType::Squiggly),
        _ => None,
    }
}

fn finish_markup(
    docs: &crate::state::commands::PdfDocs,
    pdf_path: &str,
    page: OverlayPage,
    start: (f64, f64),
    end: (f64, f64),
    mode: AnnotationMode,
) -> Option<(AnnotationType, serde_json::Value, Option<String>)> {
    let at = markup_ann_type(mode)?;
    let rx = start.0.min(end.0);
    let ry = start.1.min(end.1);
    let rw = (start.0 - end.0).abs();
    let rh = (start.1 - end.1).abs();
    if rw * page.display_scale < 5.0 && rh * page.display_scale < 5.0 {
        return None;
    }
    let markup = docs.selection_markup(
        pdf_path,
        page.index,
        page.width,
        page.height,
        rx,
        ry,
        rw,
        rh,
    );
    let (geometry, content) =
        markup_geometry_from_selection(markup.as_ref(), rx, ry, rw, rh, page.width, page.height);
    Some((at, geometry, content))
}

fn finish_ink(
    pts: &[f64],
    width: u32,
    height: u32,
) -> Option<(AnnotationType, serde_json::Value, Option<String>)> {
    if pts.len() < 4 {
        return None;
    }
    let mut min_x = f64::MAX;
    let mut min_y = f64::MAX;
    let mut max_x = f64::MIN;
    let mut max_y = f64::MIN;
    for i in (0..pts.len()).step_by(2) {
        let px = pts[i];
        let py = pts[i + 1];
        min_x = min_x.min(px);
        min_y = min_y.min(py);
        max_x = max_x.max(px);
        max_y = max_y.max(py);
    }
    Some((
        AnnotationType::Ink,
        serde_json::json!({
            "x": min_x, "y": min_y,
            "width": max_x - min_x, "height": max_y - min_y,
            "page_width": width, "page_height": height,
            "points": [pts],
        }),
        None,
    ))
}

fn persist_new_annotation(
    db: Database,
    mut tabs: Signal<PdfTabManager>,
    mut undo_stack: Signal<crate::state::undo::UndoStack>,
    ann: Annotation,
) {
    spawn(async move {
        if let Ok(id) = db.insert_annotation(&ann).await {
            let mut ann = ann;
            ann.id = Some(id);
            undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Create(ann.clone())));
            tabs.with_mut(|m| m.tab_mut().annotations.push(ann));
        }
    });
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
    let tabs = use_context::<Signal<PdfTabManager>>();
    let mut tools = use_context::<Signal<ViewerToolState>>();
    let docs = use_context::<PdfDocs>().get();
    let db = use_context::<Database>();
    let undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let ann_ctx = use_context::<AnnCtxState>();
    let mut citation = use_context::<CitationCardCtx>();
    let mut sel_copy = use_context::<SelCopyMenuCtx>();
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

    // Drag state shared by native text select and annotation tools (hooks must
    // run unconditionally — previously these lived inside `mode != None`).
    let mut drag_start = use_signal(|| None::<(f64, f64)>);
    let mut drag_current = use_signal(|| None::<(f64, f64)>);
    let mut ink_points = use_signal(Vec::<f64>::new);
    // Double / triple-click streak (Instant + last pixel + count).
    let mut click_streak_state = use_signal(|| None::<(std::time::Instant, f64, f64, u8)>);
    // Skip the tiny-drag mouseup clear after a multi-click select.
    let mut multi_click_select = use_signal(|| false);

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
        AnnotationMode::Highlight
        | AnnotationMode::Underline
        | AnnotationMode::StrikeOut
        | AnnotationMode::Squiggly => "crosshair",
        AnnotationMode::Note => "cell",
        AnnotationMode::Ink => "crosshair",
        AnnotationMode::Text => "text",
        AnnotationMode::None => "text",
    };

    // Bitmap is `width × height` at render_zoom (= zoom * dpr). Layout the
    // wrapper at display CSS pixels so `offsetX` matches overlay positions.
    let space = PageSpace::from_zooms(width, height, zoom, render_zoom);
    let display_w = space.display_w();
    let display_h = space.display_h();

    // Not-yet-rendered slot: render a sized placeholder with the SAME element type,
    // wrapper class, and id as a rendered page so it reserves scroll height and the
    // node type never changes when the real page arrives.
    let Some(base64_data) = base64_data else {
        return rsx! {
            div {
                class: "pdf-page-wrapper pdf-page-placeholder",
                id: "pdf-page-{page_index}",
                style: "width: {display_w}px; height: {display_h}px;",
            }
        };
    };

    let overlay_page = OverlayPage {
        index: page_index,
        width,
        height,
        display_scale: space.display_scale,
    };
    let drag_preview_rects = compute_drag_preview_rects(
        &docs,
        &pdf_path_for_cache,
        overlay_page,
        mode,
        drag_start(),
        drag_current(),
    );

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
            style: "cursor: {cursor}; width: {display_w}px; height: {display_h}px;",

            {
                let data_dir = config.read().effective_library_path();
                let src = crate::cache::page_file_url(&data_dir, &pdf_path_for_cache, page_index, mime)
                    .unwrap_or_else(|| format!("data:{mime};base64,{base64_data}"));
                rsx! {
                    img {
                        class: "pdf-page-img",
                        src: "{src}",
                        draggable: "false",
                    }
                }
            }

            for (si, (sx, sy, sw, sh)) in search_bounds.iter().enumerate() {
                {
                    let (sx, sy, sw, sh) = space.page_rect_to_display(*sx, *sy, *sw, *sh);
                    rsx! {
                        div {
                            key: "search-{page_index}-{si}",
                            style: "position: absolute; left: {sx}px; top: {sy}px; width: {sw}px; height: {sh}px; background: rgba(255, 165, 0, 0.35); pointer-events: none; z-index: 2; border-radius: 2px;",
                        }
                    }
                }
            }

            // Clickable intra-document link hotspots (citations, figures, sections).
            // Positioned by scaling the stored fractional rect to the rendered image
            // pixel size (width/height), so they track the page at any zoom.
            for (li, link) in page_links.iter().enumerate() {
                {
                    use crate::state::app_state::LinkDest;
                    let lx = link.x_frac * display_w;
                    let ly = link.y_frac * display_h;
                    let lw = link.w_frac * display_w;
                    let lh = link.h_frac * display_h;
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
                                // Import as appropriate. Rendered at the viewer so
                                // position:fixed is in viewport space.
                                let c = evt.client_coordinates();
                                sel_copy.set(None);
                                citation.set(Some(OpenCitationCard {
                                    dest: dest.clone(),
                                    x: c.x,
                                    y: c.y,
                                    tab_id,
                                }));
                            },
                        }
                    }
                }
            }

            for ann in page_annotations.iter() {
                {render_annotation(ann, ann_ctx, space)}
            }

            // Active selection (committed on mouseup) — below interactive links/annots.
            for (pi, (rx, ry, rw, rh)) in active_rects.iter().enumerate() {
                {
                    let (rx, ry, rw, rh) = space.page_rect_to_display(*rx, *ry, *rw, *rh);
                    rsx! {
                        div {
                            key: "sel-active-{page_index}-{pi}",
                            style: "position: absolute; left: {rx}px; top: {ry}px; width: {rw}px; height: {rh}px; background: {selection_color}; pointer-events: none; z-index: 2; border-radius: 2px;",
                        }
                    }
                }
            }

            // Drag preview (text select or Highlight/Underline).
            for (pi, (rx, ry, rw, rh)) in drag_preview_rects.iter().enumerate() {
                {
                    let (rx, ry, rw, rh) = space.page_rect_to_display(*rx, *ry, *rw, *rh);
                    rsx! {
                        div {
                            key: "drag-preview-{page_index}-{pi}",
                            style: "position: absolute; left: {rx}px; top: {ry}px; width: {rw}px; height: {rh}px; background: {preview_fill}; opacity: {preview_opacity}; pointer-events: none; z-index: 5; border-radius: 2px;",
                        }
                    }
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
                                sel_copy.set(Some((c.x, c.y)));
                            }
                            return;
                        }
                        if btn != Some(dioxus::html::input_data::MouseButton::Primary) {
                            return;
                        }
                        sel_copy.set(None);
                        let display = evt.element_coordinates();
                        let (px, py) = space.overlay_to_page(display.x, display.y);
                        let mut streak = 1u8;
                        click_streak_state.with_mut(|prev| {
                            streak = click_streak(prev, display.x, display.y);
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
                            apply_text_selection(
                                &mut tools,
                                page_index,
                                docs_select.selection_at_point(
                                    &path_select,
                                    page_index,
                                    width,
                                    height,
                                    px,
                                    py,
                                    mode,
                                ),
                            );
                            return;
                        }
                        multi_click_select.set(false);
                        tools.with_mut(|t| t.text_selection = None);
                        drag_start.set(Some((px, py)));
                        drag_current.set(Some((px, py)));
                    },
                    onmousemove: move |evt| {
                        if drag_start().is_some() {
                            let c = evt.element_coordinates();
                            drag_current.set(Some(space.overlay_to_page(c.x, c.y)));
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
                        let c = evt.element_coordinates();
                        let (x, y) = space.overlay_to_page(c.x, c.y);
                        if let Some(start) = drag_start() {
                            let rx = start.0.min(x);
                            let ry = start.1.min(y);
                            let rw = (start.0 - x).abs();
                            let rh = (start.1 - y).abs();
                            drag_start.set(None);
                            drag_current.set(None);
                            if rw * space.display_scale < 3.0 && rh * space.display_scale < 3.0 {
                                return;
                            }
                            apply_text_selection(
                                &mut tools,
                                page_index,
                                docs_select_up.selection_markup(
                                    &path_select_up,
                                    page_index,
                                    width,
                                    height,
                                    rx,
                                    ry,
                                    rw,
                                    rh,
                                ),
                            );
                        }
                    },
                    oncontextmenu: move |evt| {
                        evt.prevent_default();
                    },
                }
            }

            if mode != AnnotationMode::None {
                div {
                        class: "annotation-click-overlay",
                        onmousedown: move |evt| {
                            if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                            tools.with_mut(|t| t.text_selection = None);
                            let c = evt.element_coordinates();
                            let page = space.overlay_to_page(c.x, c.y);
                            if matches!(
                                mode,
                                AnnotationMode::Highlight
                                    | AnnotationMode::Underline
                                    | AnnotationMode::StrikeOut
                                    | AnnotationMode::Squiggly
                            ) {
                                drag_start.set(Some(page));
                                drag_current.set(Some(page));
                            }
                            if mode == AnnotationMode::Ink {
                                drag_start.set(Some(page));
                                ink_points.with_mut(|pts| {
                                    pts.clear();
                                    pts.push(page.0);
                                    pts.push(page.1);
                                });
                            }
                        },
                        onmousemove: move |evt| {
                            if matches!(
                                mode,
                                AnnotationMode::Highlight
                                    | AnnotationMode::Underline
                                    | AnnotationMode::StrikeOut
                                    | AnnotationMode::Squiggly
                            ) && drag_start().is_some()
                            {
                                let c = evt.element_coordinates();
                                drag_current.set(Some(space.overlay_to_page(c.x, c.y)));
                            }
                            if mode == AnnotationMode::Ink && drag_start().is_some() {
                                let c = evt.element_coordinates();
                                let (x, y) = space.overlay_to_page(c.x, c.y);
                                ink_points.with_mut(|pts| {
                                    pts.push(x);
                                    pts.push(y);
                                });
                            }
                        },
                        onmouseup: move |evt| {
                            if evt.trigger_button() != Some(dioxus::html::input_data::MouseButton::Primary) { return; }
                            let c = evt.element_coordinates();
                            let (x, y) = space.overlay_to_page(c.x, c.y);
                            let (ann_type, geometry, selected_content) = match mode {
                                AnnotationMode::Highlight
                                | AnnotationMode::Underline
                                | AnnotationMode::StrikeOut
                                | AnnotationMode::Squiggly => {
                                    let Some(start) = drag_start() else { return; };
                                    let Some(result) = finish_markup(
                                        &docs_annot,
                                        &path_annot,
                                        overlay_page,
                                        start,
                                        (x, y),
                                        mode,
                                    ) else {
                                        drag_start.set(None);
                                        drag_current.set(None);
                                        return;
                                    };
                                    result
                                }
                                AnnotationMode::Note => (
                                    AnnotationType::Note,
                                    serde_json::json!({
                                        "x": x, "y": y, "width": 24.0, "height": 24.0,
                                        "page_width": width, "page_height": height,
                                    }),
                                    Some(String::new()),
                                ),
                                AnnotationMode::Ink => {
                                    let pts = ink_points.read().clone();
                                    ink_points.with_mut(|p| p.clear());
                                    let Some(result) = finish_ink(&pts, width, height) else {
                                        drag_start.set(None);
                                        return;
                                    };
                                    result
                                }
                                AnnotationMode::Text => (
                                    AnnotationType::Text,
                                    serde_json::json!({
                                        "x": x, "y": y, "width": 150.0, "height": 20.0,
                                        "page_width": width, "page_height": height,
                                    }),
                                    Some(String::new()),
                                ),
                                AnnotationMode::None => return,
                            };
                            drag_start.set(None);
                            drag_current.set(None);
                            let now = chrono::Utc::now();
                            persist_new_annotation(
                                db.clone(),
                                tabs,
                                undo_stack,
                                Annotation {
                                    id: None,
                                    paper_id: paper_id.clone(),
                                    page: page_index as i32,
                                    ann_type,
                                    color: color.clone(),
                                    content: selected_content,
                                    geometry,
                                    created_at: now,
                                    modified_at: now,
                                },
                            );
                        },
                        onclick: move |_| {},
                }
            }
        }
    }
}

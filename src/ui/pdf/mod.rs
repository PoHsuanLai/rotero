mod annotation_panel;
pub(crate) mod annotation_render;
mod citation_card;
pub(crate) mod coords;
mod navigation;
mod page_overlay;
mod search_bar;
mod tab_bar;
mod toolbar;
mod viewer;

pub use tab_bar::PdfTabBar;
pub use viewer::PdfViewer;

pub(crate) use coords::PageSpace;
pub(crate) use page_overlay::copy_pdf_text_selection;

pub(crate) use citation_card::CitationCard;

use dioxus::prelude::*;

use crate::state::app_state::{AnnotationContextInfo, LinkDest, TabId};

pub(crate) type AnnCtxState = Signal<Option<AnnotationContextInfo>>;

/// Shared swatch palette for annotation tools, context menus, and settings.
pub(crate) const SELECTION_COLORS: &[(&str, &str)] = &[
    ("#ffff00", "Yellow"),
    ("#ff6b6b", "Red"),
    ("#51cf66", "Green"),
    ("#339af0", "Blue"),
    ("#cc5de8", "Purple"),
    ("#ff922b", "Orange"),
];

/// Scroll the PDF pages container. `kind` is `page-down`, `page-up`, `home`, or `end`.
pub(crate) fn pdf_scroll_js(kind: &str) -> String {
    let action = match kind {
        "page-down" => "el.scrollBy({ top: el.clientHeight * 0.9, behavior: 'smooth' });",
        "page-up" => "el.scrollBy({ top: -el.clientHeight * 0.9, behavior: 'smooth' });",
        "home" => "el.scrollTo({ top: 0, behavior: 'smooth' });",
        "end" => "el.scrollTo({ top: el.scrollHeight, behavior: 'smooth' });",
        _ => "el.scrollBy({ top: el.clientHeight * 0.9, behavior: 'smooth' });",
    };
    format!("let el = document.getElementById('pdf-pages-container'); {action}")
}

/// Citation preview, rendered at the viewer (not inside a page wrapper) so
/// `position: fixed` is in viewport space.
#[derive(Clone)]
pub(crate) struct OpenCitationCard {
    pub dest: LinkDest,
    pub x: f64,
    pub y: f64,
    pub tab_id: TabId,
}

pub(crate) type CitationCardCtx = Signal<Option<OpenCitationCard>>;

/// Copy menu for the active PDF text selection. Same viewport-space reason.
pub(crate) type SelCopyMenuCtx = Signal<Option<(f64, f64)>>;

/// While true, the viewer's scroll handler must not re-center the render
/// window. Jump-to sets this so a long citation jump (p.1 → references)
/// is not immediately undone by `onscroll` still seeing the old page.
#[derive(Clone, Copy)]
pub(crate) struct PdfScrollLock(pub Signal<bool>);

/// Builds JS that scrolls the given page into view, polling for the element so it
/// works even when the page was just added to the sliding render window and Dioxus
/// hasn't flushed it to the DOM yet. `block` is the `scrollIntoView` block alignment
/// ("start" or "center"). Retries for ~1s before giving up.
pub(crate) fn scroll_to_page_js(page_index: u32, block: &str) -> String {
    format!(
        "(function() {{ \
           let tries = 0; \
           function go() {{ \
             let el = document.getElementById('pdf-page-{page_index}'); \
             if (el) {{ el.scrollIntoView({{ behavior: 'smooth', block: '{block}' }}); return; }} \
             if (tries++ < 20) setTimeout(go, 50); \
           }} \
           go(); \
         }})()"
    )
}

/// Like [`scroll_to_page_js`] but lands at a fractional y position (0 = top,
/// 1 = bottom) down the target page, so a link jump reaches the cited line
/// rather than just the page top. The scroll container is `#pdf-pages-container`.
/// Instant jump used by the citation card. Sends `true`/`false` via `dioxus.send`
/// so Rust can wait until the scroll has actually been applied. Smooth scrolling
/// here races the viewer's `onscroll` handler, which still sees the old page
/// and recenters the render window back to where the reader was.
pub(crate) fn jump_to_page_js(page_index: u32, y_frac: Option<f64>) -> String {
    let y = y_frac.unwrap_or(0.0).clamp(0.0, 1.0);
    format!(
        "(function() {{ \
           let tries = 0; \
           function go() {{ \
             let el = document.getElementById('pdf-page-{page_index}'); \
             let cont = document.getElementById('pdf-pages-container'); \
             if (el && cont && el.offsetHeight > 0) {{ \
               let contRect = cont.getBoundingClientRect(); \
               let elRect = el.getBoundingClientRect(); \
               let target = cont.scrollTop + (elRect.top - contRect.top) + elRect.height * {y} - contRect.height * 0.15; \
               cont.scrollTo(0, Math.max(0, target)); \
               dioxus.send(true); \
               return; \
             }} \
             if (tries++ < 40) setTimeout(go, 50); \
             else dioxus.send(false); \
           }} \
           go(); \
         }})()"
    )
}

pub(crate) fn hex_to_rgba(hex: &str, alpha: f32) -> String {
    let hex = hex.trim_start_matches('#');
    // The ASCII check is what makes the slices safe: a colour string is stored
    // data and may be anything, and six *bytes* of a multi-byte character would
    // have panicked here inside a render path.
    if hex.len() >= 6 && hex.as_bytes()[..6].is_ascii() {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        format!("rgba({r}, {g}, {b}, {alpha})")
    } else {
        format!("rgba(0, 100, 255, {alpha})")
    }
}

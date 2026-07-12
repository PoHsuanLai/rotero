mod annotation_panel;
pub(crate) mod annotation_render;
mod navigation;
mod page_overlay;
mod search_bar;
mod tab_bar;
mod toolbar;
mod viewer;

pub use tab_bar::PdfTabBar;
pub use viewer::PdfViewer;

use dioxus::prelude::*;

use crate::state::app_state::AnnotationContextInfo;

pub(crate) type AnnCtxState = Signal<Option<AnnotationContextInfo>>;

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
pub(crate) fn scroll_to_page_at_js(page_index: u32, y_frac: f64) -> String {
    let y_frac = y_frac.clamp(0.0, 1.0);
    format!(
        "(function() {{ \
           let tries = 0; \
           function go() {{ \
             let el = document.getElementById('pdf-page-{page_index}'); \
             let cont = document.getElementById('pdf-pages-container'); \
             if (el && cont) {{ \
               let contRect = cont.getBoundingClientRect(); \
               let elRect = el.getBoundingClientRect(); \
               let target = cont.scrollTop + (elRect.top - contRect.top) + elRect.height * {y_frac} - contRect.height * 0.15; \
               cont.scrollTo({{ top: Math.max(0, target), behavior: 'smooth' }}); \
               return; \
             }} \
             if (tries++ < 20) setTimeout(go, 50); \
           }} \
           go(); \
         }})()"
    )
}

pub(crate) fn hex_to_rgba(hex: &str, alpha: f32) -> String {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        format!("rgba({r}, {g}, {b}, {alpha})")
    } else {
        format!("rgba(0, 100, 255, {alpha})")
    }
}

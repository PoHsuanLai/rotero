mod annotation_panel;
pub(crate) mod annotation_render;
mod navigation;
pub(crate) mod page_image;
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

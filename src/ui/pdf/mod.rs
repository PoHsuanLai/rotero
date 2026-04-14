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

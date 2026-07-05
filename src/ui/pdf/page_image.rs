use std::sync::Arc;

use dioxus::prelude::*;

/// The single source of truth for rendering a rasterized PDF page crisply.
///
/// pdfium rasterizes each page at high resolution (`render_zoom = zoom * dpr`).
/// The `<img>` carries the raw pixel `width`/`height`, and the caller scales it
/// down to display size via CSS `zoom = zoom / render_zoom` on a wrapper. Unlike
/// `transform: scale()`, CSS `zoom` affects layout, so pages don't overlap.
///
/// Both the interactive paper viewer (`PdfPageWithOverlay`) and the read-only
/// document preview use this so the rendering technique lives in one place.
#[component]
pub(crate) fn PdfPageImage(
    base64_data: Arc<String>,
    mime: &'static str,
    /// Raw rasterized pixel width (at render_zoom).
    width: u32,
    /// Raw rasterized pixel height (at render_zoom).
    height: u32,
    /// Optional pre-built image `src` (e.g. an on-disk cache URL). Falls back to
    /// an inline `data:` URL built from `base64_data` when `None`.
    #[props(default)]
    src: Option<String>,
) -> Element {
    let src = src.unwrap_or_else(|| format!("data:{mime};base64,{base64_data}"));
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

/// Compute the CSS `zoom` factor that scales a page rasterized at `render_zoom`
/// down to the display `zoom`. Shared so callers agree on the math.
pub(crate) fn css_zoom(zoom: f32, render_zoom: f32) -> f32 {
    if render_zoom > 0.0 { zoom / render_zoom } else { 1.0 }
}

//! Read-only PDF preview pane for a compiled document.
//!
//! Unlike the main `PdfViewer` (coupled to `PdfTabManager`'s active tab), this
//! renders an arbitrary PDF file path by driving the path-addressable render
//! backend directly (`RenderChannel` + `RenderRequest::OpenPdf`) and emitting
//! plain `<img>` elements — no tabs, annotations, or search overlay.

use dioxus::prelude::*;

use crate::app::{DevicePixelRatio, RenderChannel};
use crate::state::app_state::RenderedPageData;
use crate::state::commands::RenderRequest;
use crate::sync::engine::SyncConfig;

/// Render every page of `pdf_path` as base64 images. Recompiles whenever the
/// path or `reload` token changes.
///
/// The props are `ReadOnlySignal` so that reading them inside the `use_resource`
/// closure subscribes it: callers still pass plain values (auto-converted), but
/// the resource re-runs when a compile bumps `reload` or the path changes.
#[component]
pub(super) fn DocumentPreview(
    pdf_path: ReadSignal<Option<String>>,
    reload: ReadSignal<u64>,
) -> Element {
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<SyncConfig>>();
    let dpr_sig = use_context::<Signal<DevicePixelRatio>>();

    // Rasterize at high resolution (display zoom * device pixel ratio) so the
    // responsive page images stay crisp when scaled to fill the pane width.
    let render_scale = config.read().pdf.default_zoom * dpr_sig.read().0;
    let batch_size = config.read().pdf.page_batch_size;

    let pages = use_resource(move || {
        let sender = render_ch.sender();
        // Reactive reads INSIDE the tracked closure body so the resource
        // re-runs when either changes.
        let path = pdf_path();
        let _token = reload();
        async move {
            let path = path?;
            if !std::path::Path::new(&path).exists() {
                return None;
            }
            let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
            sender
                .send(RenderRequest::OpenPdf {
                    pdf_path: path,
                    zoom: render_scale,
                    batch_size,
                    reply: reply_tx,
                })
                .ok()?;
            let (_count, pages): (u32, Vec<RenderedPageData>) = reply_rx.await.ok()?.ok()?;
            Some(pages)
        }
    });

    rsx! {
        div { class: "document-preview",
            match &*pages.read_unchecked() {
                Some(Some(list)) if !list.is_empty() => rsx! {
                    for page in list.clone() {
                        PreviewPage { page: page.clone() }
                    }
                },
                Some(Some(_)) | Some(None) => rsx! {
                    div { class: "document-preview-empty",
                        i { class: "bi bi-file-earmark-text" }
                        p { "Nothing to preview yet." }
                        p { class: "document-preview-hint", "Click Compile to typeset this document." }
                    }
                },
                None => rsx! { div { class: "document-preview-empty", "Rendering…" } },
            }
        }
    }
}

#[component]
fn PreviewPage(page: RenderedPageData) -> Element {
    // The page image is rasterized at high resolution (zoom * dpr); here we let
    // it scale responsively to fill the pane width (`.document-preview-page img`
    // sets width:100%). Downscaling a high-res source stays crisp, so — unlike
    // the fixed-size paper viewer — no CSS `zoom` wrapper is needed.
    let src = format!("data:{};base64,{}", page.mime, page.base64_data);
    rsx! {
        div { class: "document-preview-page",
            img {
                class: "pdf-page-img",
                src: "{src}",
                draggable: "false",
            }
        }
    }
}

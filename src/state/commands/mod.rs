mod citations;
mod import;
mod library_helpers;
mod pdf_cache;
mod pdf_extract;
mod pdf_loading;

pub use citations::*;
pub use import::*;
#[cfg(test)]
mod import_queue_test;
pub use library_helpers::*;
pub use pdf_cache::*;
pub use pdf_extract::*;
pub use pdf_loading::*;

use std::collections::HashMap;
use std::sync::mpsc;

use rotero_pdf::PageTextData;
use tokio::sync::oneshot;

use super::app_state::RenderedPageData;

pub type PdfExtractResult = (Vec<(u32, String)>, rotero_pdf::PdfDocMetadata);

pub enum RenderRequest {
    OpenPdf {
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
        reply: oneshot::Sender<Result<(u32, Vec<RenderedPageData>), String>>,
    },
    RenderMorePages {
        pdf_path: String,
        start: u32,
        count: u32,
        zoom: f32,
        reply: oneshot::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    ExtractText {
        pdf_path: String,
        page_dims: Vec<(u32, u32, u32)>,
        reply: oneshot::Sender<Result<HashMap<u32, PageTextData>, String>>,
    },
    RenderThumbnails {
        pdf_path: String,
        start: u32,
        count: u32,
        reply: oneshot::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    ExtractOutline {
        pdf_path: String,
        reply: oneshot::Sender<Result<Vec<rotero_pdf::BookmarkEntry>, String>>,
    },
    GetPageDimensions {
        pdf_path: String,
        reply: oneshot::Sender<Result<Vec<(f32, f32)>, String>>,
    },
    ExtractMetadataText {
        pdf_path: String,
        page_count: u32,
        reply: oneshot::Sender<Result<PdfExtractResult, String>>,
    },
    ExtractAnnotations {
        pdf_path: String,
        reply: oneshot::Sender<Result<Vec<rotero_pdf::ExtractedAnnotation>, String>>,
    },
    ExtractLinks {
        pdf_path: String,
        reply: oneshot::Sender<Result<Vec<rotero_pdf::ExtractedLink>, String>>,
    },
    /// Drops the engine's cached PDF file bytes. Sent when the last PDF tab
    /// closes so a large document's raw bytes aren't pinned indefinitely.
    ClearCache,
}

/// Publishes why PDFium could not be loaded, or `None` while it is fine.
///
/// Set from the render thread before it starts draining, so the startup
/// preflight can report the real reason instead of the user meeting a dead PDF
/// pane with the explanation buried in a log file.
pub static PDF_ENGINE_ERROR: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Resolves once the render thread has finished trying to bind PDFium.
///
/// Lets the preflight read [`PDF_ENGINE_ERROR`] at a defined point rather than
/// racing the bind — the thread starts after the window launches, so a bare read
/// at startup would usually run first and find nothing.
pub static PDF_ENGINE_READY: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Reply to every request with the same error, forever.
///
/// Returning from the thread instead would drop the receiver, and every later
/// `send` would fail with "sending on a closed channel" — which is what the ~20
/// call sites in `src/ui/pdf/` used to surface. Staying alive means each one
/// gets the actual reason, and none of them need to change.
fn drain_with_error(rx: mpsc::Receiver<RenderRequest>, message: String) {
    while let Ok(req) = rx.recv() {
        // Each reply channel carries a different success type, so the error has
        // to be built per arm rather than shared.
        match req {
            RenderRequest::OpenPdf { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::RenderMorePages { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::ExtractText { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::RenderThumbnails { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::ExtractOutline { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::GetPageDimensions { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::ExtractMetadataText { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::ExtractAnnotations { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::ExtractLinks { reply, .. } => {
                let _ = reply.send(Err(message.clone()));
            }
            RenderRequest::ClearCache => {}
        }
    }
}

pub fn spawn_render_thread() -> mpsc::Sender<RenderRequest> {
    let (tx, rx) = mpsc::channel::<RenderRequest>();

    std::thread::spawn(move || {
        #[cfg(feature = "pdfium-static")]
        let engine_result = rotero_pdf::PdfEngine::new_static();
        #[cfg(not(feature = "pdfium-static"))]
        let engine_result = rotero_pdf::PdfEngine::new(None);
        let mut engine = match engine_result {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("Failed to bind PDFium: {e}");
                // The resolver's message names every path it tried, which is the
                // information needed to fix a broken install.
                let message = format!("PDF engine unavailable: {e}");
                let _ = PDF_ENGINE_ERROR.set(message.clone());
                let _ = PDF_ENGINE_READY.set(());
                drain_with_error(rx, message);
                return;
            }
        };
        let _ = PDF_ENGINE_READY.set(());

        while let Ok(req) = rx.recv() {
            match req {
                RenderRequest::OpenPdf {
                    pdf_path,
                    zoom,
                    batch_size,
                    reply,
                } => {
                    let result = (|| {
                        let (page_count, rendered) = engine
                            .open_and_render_initial(&pdf_path, zoom, batch_size)
                            .map_err(|e| e.to_string())?;
                        let pages: Vec<RenderedPageData> =
                            rendered.into_iter().map(|r| r.into()).collect();
                        Ok((page_count, pages))
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::RenderMorePages {
                    pdf_path,
                    start,
                    count,
                    zoom,
                    reply,
                } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, start, count, zoom)
                            .map_err(|e| e.to_string())?;
                        Ok(rendered
                            .into_iter()
                            .map(|r| r.into())
                            .collect::<Vec<RenderedPageData>>())
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractText {
                    pdf_path,
                    page_dims,
                    reply,
                } => {
                    let result = (|| {
                        let text_pages = rotero_pdf::text_extract::extract_pages_text(
                            engine.pdfium(),
                            &pdf_path,
                            &page_dims,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(text_pages
                            .into_iter()
                            .map(|t| (t.page_index, t))
                            .collect::<HashMap<u32, PageTextData>>())
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::RenderThumbnails {
                    pdf_path,
                    start,
                    count,
                    reply,
                } => {
                    let result = (|| {
                        let rendered = engine
                            .render_thumbnails_range(&pdf_path, start, count, 120)
                            .map_err(|e| e.to_string())?;
                        Ok(rendered
                            .into_iter()
                            .map(|r| r.into())
                            .collect::<Vec<RenderedPageData>>())
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractOutline { pdf_path, reply } => {
                    let result = engine.extract_outline(&pdf_path).map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
                RenderRequest::GetPageDimensions { pdf_path, reply } => {
                    let result = engine
                        .get_page_dimensions(&pdf_path)
                        .map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractMetadataText {
                    pdf_path,
                    page_count,
                    reply,
                } => {
                    let result = (|| {
                        let indices: Vec<u32> = (0..page_count).collect();
                        let raw_text = rotero_pdf::text_extract::extract_raw_text(
                            engine.pdfium(),
                            &pdf_path,
                            &indices,
                        )
                        .map_err(|e| e.to_string())?;
                        let doc_meta = rotero_pdf::text_extract::extract_doc_metadata(
                            engine.pdfium(),
                            &pdf_path,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok((raw_text, doc_meta))
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractAnnotations { pdf_path, reply } => {
                    let result = engine
                        .extract_annotations(&pdf_path)
                        .map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractLinks { pdf_path, reply } => {
                    let result = engine.extract_links(&pdf_path).map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
                RenderRequest::ClearCache => {
                    engine.clear_byte_cache();
                }
            }
        }
    });

    tx
}

pub(crate) async fn recv_reply<T: Send + 'static>(
    rx: oneshot::Receiver<Result<T, String>>,
) -> Result<T, String> {
    rx.await.map_err(|e| e.to_string())?
}

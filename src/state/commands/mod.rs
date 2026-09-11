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
use std::sync::Arc;

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
    /// Drops the shared engine's cached `Arc<Document>`. Sent when the last PDF
    /// tab closes so a large document isn't pinned indefinitely. In-flight
    /// workers keep their own `Arc` until they finish.
    ClearCache,
}

/// Max concurrent blocking PDF workers (render / extract).
fn render_concurrency() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().clamp(2, 8))
        .unwrap_or(4)
}

fn handle_render_request(engine: &rotero_pdf::PdfEngine, req: RenderRequest) {
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
                let pages: Vec<RenderedPageData> = rendered.into_iter().map(|r| r.into()).collect();
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
                let doc = engine.document(&pdf_path).map_err(|e| e.to_string())?;
                let text_pages =
                    rotero_pdf::text_extract::extract_pages_text(&doc, &page_dims)
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
                let doc = engine.document(&pdf_path).map_err(|e| e.to_string())?;
                let indices: Vec<u32> = (0..page_count).collect();
                let raw_text = rotero_pdf::text_extract::extract_raw_text(&doc, &indices)
                    .map_err(|e| e.to_string())?;
                let doc_meta = rotero_pdf::text_extract::extract_doc_metadata(&doc);
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
            // Normally handled on the dispatcher; safe if a worker ever sees it.
            engine.clear_cache();
        }
    }
}

/// Spawns the PDF render pool and returns a channel façade for UI call sites.
///
/// pdfrum documents are `Send + Sync`, so work runs on `tokio::task::spawn_blocking`
/// workers capped by a semaphore. Each blocking job uses the shared
/// [`rotero_pdf::PdfEngine`] cache (`Arc<Document>`) and creates its own
/// `RenderSession`. The `RenderRequest` / oneshot reply pattern is unchanged.
pub fn spawn_render_pool() -> mpsc::Sender<RenderRequest> {
    let (tx, rx) = mpsc::channel::<RenderRequest>();

    std::thread::Builder::new()
        .name("pdf-render-dispatch".into())
        .spawn(move || {
            let engine = Arc::new(rotero_pdf::PdfEngine::new());
            let concurrency = render_concurrency();
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("pdf-render-rt")
                .enable_all()
                .build()
                .expect("pdf render tokio runtime");
            let sem = Arc::new(tokio::sync::Semaphore::new(concurrency));

            while let Ok(req) = rx.recv() {
                match req {
                    RenderRequest::ClearCache => {
                        engine.clear_cache();
                    }
                    req => {
                        let engine = Arc::clone(&engine);
                        let sem = Arc::clone(&sem);
                        rt.spawn(async move {
                            let Ok(_permit) = sem.acquire().await else {
                                return;
                            };
                            let result = tokio::task::spawn_blocking(move || {
                                handle_render_request(&engine, req);
                            })
                            .await;
                            if let Err(e) = result {
                                tracing::error!("pdf render worker join error: {e}");
                            }
                        });
                    }
                }
            }
        })
        .expect("spawn pdf render dispatcher");

    tx
}

/// Backward-compatible alias for [`spawn_render_pool`].
pub fn spawn_render_thread() -> mpsc::Sender<RenderRequest> {
    spawn_render_pool()
}

pub(crate) async fn recv_reply<T: Send + 'static>(
    rx: oneshot::Receiver<Result<T, String>>,
) -> Result<T, String> {
    rx.await.map_err(|e| e.to_string())?
}

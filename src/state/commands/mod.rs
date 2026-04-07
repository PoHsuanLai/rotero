mod pdf_cache;
mod pdf_extract;
mod pdf_loading;

pub use pdf_cache::*;
pub use pdf_extract::*;
pub use pdf_loading::*;

use std::collections::HashMap;
use std::sync::mpsc;

use rotero_pdf::PageTextData;

use super::app_state::RenderedPageData;

/// Result type for PDF text/metadata extraction.
pub type PdfExtractResult = (Vec<(u32, String)>, rotero_pdf::PdfDocMetadata);

/// A request to the PDF render thread.
pub enum RenderRequest {
    OpenPdf {
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
        reply: mpsc::Sender<Result<(u32, Vec<RenderedPageData>), String>>,
    },
    RenderMorePages {
        pdf_path: String,
        start: u32,
        count: u32,
        zoom: f32,
        reply: mpsc::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    SetZoom {
        pdf_path: String,
        page_count: u32,
        new_zoom: f32,
        reply: mpsc::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    ExtractText {
        pdf_path: String,
        page_dims: Vec<(u32, u32, u32)>,
        reply: mpsc::Sender<Result<HashMap<u32, PageTextData>, String>>,
    },
    RenderThumbnails {
        pdf_path: String,
        start: u32,
        count: u32,
        reply: mpsc::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    ExtractOutline {
        pdf_path: String,
        reply: mpsc::Sender<Result<Vec<rotero_pdf::BookmarkEntry>, String>>,
    },
    GetPageDimensions {
        pdf_path: String,
        reply: mpsc::Sender<Result<Vec<(f32, f32)>, String>>,
    },
    ExtractMetadataText {
        pdf_path: String,
        page_count: u32,
        reply: mpsc::Sender<Result<PdfExtractResult, String>>,
    },
    ExtractAnnotations {
        pdf_path: String,
        reply: mpsc::Sender<Result<Vec<rotero_pdf::ExtractedAnnotation>, String>>,
    },
}

/// Spawn a dedicated thread that owns the PdfEngine and processes render requests.
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
                eprintln!("Failed to bind PDFium: {e}");
                return;
            }
        };

        while let Ok(req) = rx.recv() {
            match req {
                RenderRequest::OpenPdf {
                    pdf_path,
                    zoom,
                    batch_size,
                    reply,
                } => {
                    let result = (|| {
                        let info = engine.load_document(&pdf_path).map_err(|e| e.to_string())?;
                        let render_count = info.page_count.min(batch_size);
                        let rendered = engine
                            .render_pages(&pdf_path, 0, render_count, zoom)
                            .map_err(|e| e.to_string())?;
                        let pages: Vec<RenderedPageData> =
                            rendered.into_iter().map(|r| r.into()).collect();
                        Ok((info.page_count, pages))
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
                RenderRequest::SetZoom {
                    pdf_path,
                    page_count,
                    new_zoom,
                    reply,
                } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, 0, page_count, new_zoom)
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
            }
        }
    });

    tx
}

pub(crate) async fn recv_reply<T: Send + 'static>(
    rx: mpsc::Receiver<Result<T, String>>,
) -> Result<T, String> {
    tokio::task::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
}

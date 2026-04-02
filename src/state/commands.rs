use std::collections::HashMap;
use std::sync::mpsc;

use dioxus::prelude::*;
use rotero_pdf::PageTextData;

use super::app_state::{PdfViewState, RenderedPageData};

/// Result of rendering pages: page data + text data.
pub struct RenderResult {
    pub pages: Vec<RenderedPageData>,
    pub text_data: HashMap<u32, PageTextData>,
}

/// A request to the PDF render thread.
pub enum RenderRequest {
    OpenPdf {
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
        reply: mpsc::Sender<Result<(u32, RenderResult), String>>,
    },
    RenderMorePages {
        pdf_path: String,
        start: u32,
        count: u32,
        zoom: f32,
        reply: mpsc::Sender<Result<RenderResult, String>>,
    },
    SetZoom {
        pdf_path: String,
        page_count: u32,
        new_zoom: f32,
        reply: mpsc::Sender<Result<RenderResult, String>>,
    },
    RenderThumbnails {
        pdf_path: String,
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
}

/// Spawn a dedicated thread that owns the PdfEngine and processes render requests.
/// Returns a sender for submitting requests.
pub fn spawn_render_thread() -> mpsc::Sender<RenderRequest> {
    let (tx, rx) = mpsc::channel::<RenderRequest>();

    std::thread::spawn(move || {
        let engine = match rotero_pdf::PdfEngine::new(None) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to bind PDFium: {e}");
                return;
            }
        };

        while let Ok(req) = rx.recv() {
            match req {
                RenderRequest::OpenPdf { pdf_path, zoom, batch_size, reply } => {
                    let result = (|| {
                        let info = engine.load_document(&pdf_path).map_err(|e| e.to_string())?;
                        let render_count = info.page_count.min(batch_size);
                        let rendered = engine
                            .render_pages(&pdf_path, 0, render_count, zoom)
                            .map_err(|e| e.to_string())?;
                        let pages: Vec<RenderedPageData> =
                            rendered.into_iter().map(|r| r.into()).collect();

                        // Extract text for rendered pages
                        let text_pages = rotero_pdf::text_extract::extract_pages_text(
                            engine.pdfium(), &pdf_path, 0, render_count, zoom,
                        ).unwrap_or_default();
                        let text_data: HashMap<u32, PageTextData> =
                            text_pages.into_iter().map(|t| (t.page_index, t)).collect();

                        Ok((info.page_count, RenderResult { pages, text_data }))
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::RenderMorePages { pdf_path, start, count, zoom, reply } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, start, count, zoom)
                            .map_err(|e| e.to_string())?;
                        let pages: Vec<RenderedPageData> =
                            rendered.into_iter().map(|r| r.into()).collect();

                        let text_pages = rotero_pdf::text_extract::extract_pages_text(
                            engine.pdfium(), &pdf_path, start, count, zoom,
                        ).unwrap_or_default();
                        let text_data: HashMap<u32, PageTextData> =
                            text_pages.into_iter().map(|t| (t.page_index, t)).collect();

                        Ok(RenderResult { pages, text_data })
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::SetZoom { pdf_path, page_count, new_zoom, reply } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, 0, page_count, new_zoom)
                            .map_err(|e| e.to_string())?;
                        let pages: Vec<RenderedPageData> =
                            rendered.into_iter().map(|r| r.into()).collect();

                        let text_pages = rotero_pdf::text_extract::extract_pages_text(
                            engine.pdfium(), &pdf_path, 0, page_count, new_zoom,
                        ).unwrap_or_default();
                        let text_data: HashMap<u32, PageTextData> =
                            text_pages.into_iter().map(|t| (t.page_index, t)).collect();

                        Ok(RenderResult { pages, text_data })
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::RenderThumbnails { pdf_path, reply } => {
                    let result = (|| {
                        let rendered = engine
                            .render_all_thumbnails(&pdf_path, 120)
                            .map_err(|e| e.to_string())?;
                        Ok(rendered.into_iter().map(|r| r.into()).collect::<Vec<RenderedPageData>>())
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractOutline { pdf_path, reply } => {
                    let result = engine
                        .extract_outline(&pdf_path)
                        .map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
                RenderRequest::GetPageDimensions { pdf_path, reply } => {
                    let result = engine
                        .get_page_dimensions(&pdf_path)
                        .map_err(|e| e.to_string());
                    let _ = reply.send(result);
                }
            }
        }
    });

    tx
}

/// Open a PDF file and render its first batch of pages (async, off main thread).
pub async fn open_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    state: &mut Signal<PdfViewState>,
    pdf_path: &str,
) -> Result<(), String> {
    let zoom = state.read().zoom;
    let batch_size = state.read().page_batch_size.unwrap_or(5);
    let path = pdf_path.to_string();

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::OpenPdf {
            pdf_path: path.clone(),
            zoom,
            batch_size,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let (page_count, result) = tokio::task::spawn_blocking(move || reply_rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())??;

    state.set(PdfViewState {
        pdf_path: Some(path),
        page_count,
        current_page: 0,
        zoom,
        render_zoom: zoom,
        rendered_pages: result.pages,
        text_data: result.text_data,
        ..PdfViewState::new()
    });

    Ok(())
}

/// Render additional pages for lazy loading on scroll (async, off main thread).
pub async fn render_more_pages(
    render_tx: &mpsc::Sender<RenderRequest>,
    state: &mut Signal<PdfViewState>,
    start: u32,
    count: u32,
) -> Result<(), String> {
    let s = state.read();
    let pdf_path = s.pdf_path.clone();
    let zoom = s.zoom;
    drop(s);

    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderMorePages {
            pdf_path,
            start,
            count,
            zoom,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let result = tokio::task::spawn_blocking(move || reply_rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())??;

    state.with_mut(|s| {
        s.rendered_pages.extend(result.pages);
        s.text_data.extend(result.text_data);
    });

    Ok(())
}

/// Change zoom level and re-render all loaded pages (async, off main thread).
pub async fn set_zoom(
    render_tx: &mpsc::Sender<RenderRequest>,
    state: &mut Signal<PdfViewState>,
    new_zoom: f32,
) -> Result<(), String> {
    let s = state.read();
    let pdf_path = s.pdf_path.clone();
    let page_count = s.rendered_pages.len() as u32;
    drop(s);

    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    // Set zoom immediately for CSS progressive scaling
    state.with_mut(|s| s.zoom = new_zoom);

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::SetZoom {
            pdf_path,
            page_count,
            new_zoom,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let result = tokio::task::spawn_blocking(move || reply_rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())??;

    state.with_mut(|s| {
        s.render_zoom = new_zoom;
        s.rendered_pages = result.pages;
        s.text_data = result.text_data;
    });

    Ok(())
}

/// Load thumbnails for all pages (async, off main thread).
pub async fn load_thumbnails(
    render_tx: &mpsc::Sender<RenderRequest>,
    state: &mut Signal<PdfViewState>,
) -> Result<(), String> {
    let pdf_path = state.read().pdf_path.clone();
    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderThumbnails {
            pdf_path,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let thumbnails = tokio::task::spawn_blocking(move || reply_rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())??;

    state.with_mut(|s| s.thumbnails = thumbnails);
    Ok(())
}

/// Extract outline/bookmarks (async, off main thread).
pub async fn load_outline(
    render_tx: &mpsc::Sender<RenderRequest>,
    state: &mut Signal<PdfViewState>,
) -> Result<(), String> {
    let pdf_path = state.read().pdf_path.clone();
    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::ExtractOutline {
            pdf_path,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let outline = tokio::task::spawn_blocking(move || reply_rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())??;

    state.with_mut(|s| s.outline = outline);
    Ok(())
}

/// Get page dimensions for virtualization (async, off main thread).
pub async fn load_page_dimensions(
    render_tx: &mpsc::Sender<RenderRequest>,
    state: &mut Signal<PdfViewState>,
) -> Result<(), String> {
    let pdf_path = state.read().pdf_path.clone();
    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::GetPageDimensions {
            pdf_path,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let dims = tokio::task::spawn_blocking(move || reply_rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())??;

    state.with_mut(|s| s.page_dimensions = dims);
    Ok(())
}

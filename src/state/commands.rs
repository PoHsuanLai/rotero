use std::collections::HashMap;
use std::sync::mpsc;

use dioxus::prelude::*;
use rotero_pdf::PageTextData;

use super::app_state::{PdfTabManager, RenderedPageData, TabId};

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
        start: u32,
        count: u32,
        zoom: f32,
        reply: mpsc::Sender<Result<HashMap<u32, PageTextData>, String>>,
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
                        Ok((info.page_count, pages))
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::RenderMorePages { pdf_path, start, count, zoom, reply } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, start, count, zoom)
                            .map_err(|e| e.to_string())?;
                        Ok(rendered.into_iter().map(|r| r.into()).collect::<Vec<RenderedPageData>>())
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::SetZoom { pdf_path, page_count, new_zoom, reply } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, 0, page_count, new_zoom)
                            .map_err(|e| e.to_string())?;
                        Ok(rendered.into_iter().map(|r| r.into()).collect::<Vec<RenderedPageData>>())
                    })();
                    let _ = reply.send(result);
                }
                RenderRequest::ExtractText { pdf_path, start, count, zoom, reply } => {
                    let result = (|| {
                        let text_pages = rotero_pdf::text_extract::extract_pages_text(
                            engine.pdfium(), &pdf_path, start, count, zoom,
                        ).map_err(|e| e.to_string())?;
                        Ok(text_pages.into_iter().map(|t| (t.page_index, t)).collect::<HashMap<u32, PageTextData>>())
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

// ── Helper: wait for render thread reply ──────────────────────────

async fn recv_reply<T: Send + 'static>(rx: mpsc::Receiver<Result<T, String>>) -> Result<T, String> {
    tokio::task::spawn_blocking(move || rx.recv())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
}

// ── Tab-aware async commands ──────────────────────────────────────

/// Open/render a PDF tab's first batch of pages.
/// Uses disk cache when available for instant loading.
pub async fn open_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    let (path, zoom, batch_size) = {
        let mgr = tabs.read();
        let tab = mgr.tabs.iter().find(|t| t.id == tab_id).ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.view.zoom, tab.view.page_batch_size)
    };

    // Try loading from disk cache first
    if let Some((meta, cached_pages)) = crate::cache::load_cached(data_dir, &path, zoom) {
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.page_count = meta.page_count;
                tab.view.render_zoom = zoom;
                tab.render.rendered_pages = cached_pages;
                tab.is_loading = false;
            }
        });
        // Load cached text too
        if let Some(text_data) = crate::cache::load_cached_text(data_dir, &path) {
            tabs.with_mut(|mgr| {
                if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.render.text_data = text_data;
                }
            });
        }
        return Ok(());
    }

    // Cache miss — render via PDFium
    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::OpenPdf {
            pdf_path: path.clone(),
            zoom,
            batch_size,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let (page_count, pages) = recv_reply(reply_rx).await?;

    // Save to cache in background
    let cache_dir = data_dir.to_path_buf();
    let cache_path = path.clone();
    let cache_pages = pages.clone();
    std::thread::spawn(move || {
        crate::cache::save_pages(&cache_dir, &cache_path, zoom, page_count, &cache_pages);
    });

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.page_count = page_count;
            tab.view.render_zoom = zoom;
            tab.render.rendered_pages = pages;
            tab.is_loading = false;
        }
    });

    // Extract text in background
    let batch = page_count.min(batch_size);
    let (text_tx, text_rx) = mpsc::channel();
    let _ = render_tx.send(RenderRequest::ExtractText {
        pdf_path: path.clone(),
        start: 0,
        count: batch,
        zoom,
        reply: text_tx,
    });
    if let Ok(text_data) = recv_reply(text_rx).await {
        // Save text cache
        let cache_dir = data_dir.to_path_buf();
        let cache_path = path.clone();
        let text_clone = text_data.clone();
        std::thread::spawn(move || {
            crate::cache::save_text(&cache_dir, &cache_path, &text_clone);
        });

        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.render.text_data = text_data;
            }
        });
    }

    Ok(())
}

/// Render additional pages for lazy loading on scroll.
pub async fn render_more_pages(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    start: u32,
    count: u32,
) -> Result<(), String> {
    let (pdf_path, zoom) = {
        let mgr = tabs.read();
        let tab = mgr.tabs.iter().find(|t| t.id == tab_id).ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.view.zoom)
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderMorePages {
            pdf_path: pdf_path.clone(),
            start,
            count,
            zoom,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let pages = recv_reply(reply_rx).await?;

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.render.rendered_pages.extend(pages);
        }
    });

    // Extract text for new pages in background
    let (text_tx, text_rx) = mpsc::channel();
    let _ = render_tx.send(RenderRequest::ExtractText {
        pdf_path,
        start,
        count,
        zoom,
        reply: text_tx,
    });
    if let Ok(text_data) = recv_reply(text_rx).await {
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.render.text_data.extend(text_data);
            }
        });
    }

    Ok(())
}

/// Change zoom level and re-render all loaded pages.
pub async fn set_zoom(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    new_zoom: f32,
) -> Result<(), String> {
    let (pdf_path, page_count) = {
        let mgr = tabs.read();
        let tab = mgr.tabs.iter().find(|t| t.id == tab_id).ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.render.rendered_pages.len() as u32)
    };

    // Set zoom immediately for CSS progressive scaling
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.view.zoom = new_zoom;
        }
    });

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::SetZoom {
            pdf_path,
            page_count,
            new_zoom,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let pages = recv_reply(reply_rx).await?;

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.view.render_zoom = new_zoom;
            tab.render.rendered_pages = pages;
            tab.render.text_data.clear(); // will be re-extracted at new zoom
        }
    });

    Ok(())
}

/// Load thumbnails for all pages.
pub async fn load_thumbnails(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
) -> Result<(), String> {
    let pdf_path = {
        let mgr = tabs.read();
        mgr.tabs.iter().find(|t| t.id == tab_id).ok_or("Tab not found")?.pdf_path.clone()
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderThumbnails { pdf_path, reply: reply_tx })
        .map_err(|e| e.to_string())?;

    let thumbnails = recv_reply(reply_rx).await?;

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.render.thumbnails = thumbnails;
        }
    });

    Ok(())
}

/// Extract outline/bookmarks.
pub async fn load_outline(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
) -> Result<(), String> {
    let pdf_path = {
        let mgr = tabs.read();
        mgr.tabs.iter().find(|t| t.id == tab_id).ok_or("Tab not found")?.pdf_path.clone()
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::ExtractOutline { pdf_path, reply: reply_tx })
        .map_err(|e| e.to_string())?;

    let outline = recv_reply(reply_rx).await?;

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.nav.outline = outline;
        }
    });

    Ok(())
}

/// Pre-cache a PDF in the background (render all pages + extract text to disk).
/// Fire-and-forget — does not update any UI state.
pub async fn precache_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    pdf_path: &str,
    data_dir: &std::path::Path,
    zoom: f32,
) {
    // Skip if already cached
    if crate::cache::load_cached(data_dir, pdf_path, zoom).is_some() {
        return;
    }

    let path = pdf_path.to_string();

    // Render first batch of pages
    let (reply_tx, reply_rx) = mpsc::channel();
    if render_tx.send(RenderRequest::OpenPdf {
        pdf_path: path.clone(),
        zoom,
        batch_size: 5,
        reply: reply_tx,
    }).is_err() {
        return;
    }

    let Ok((page_count, pages)) = recv_reply(reply_rx).await else { return };

    // Save pages to cache
    let dir = data_dir.to_path_buf();
    let p = path.clone();
    let pg = pages.clone();
    std::thread::spawn(move || {
        crate::cache::save_pages(&dir, &p, zoom, page_count, &pg);
    });

    // Extract and cache text
    let (text_tx, text_rx) = mpsc::channel();
    if render_tx.send(RenderRequest::ExtractText {
        pdf_path: path.clone(),
        start: 0,
        count: page_count.min(5),
        zoom,
        reply: text_tx,
    }).is_err() {
        return;
    }

    if let Ok(text_data) = recv_reply(text_rx).await {
        let dir = data_dir.to_path_buf();
        std::thread::spawn(move || {
            crate::cache::save_text(&dir, &path, &text_data);
        });
    }
}

use std::collections::HashMap;
use std::sync::mpsc;

use dioxus::prelude::*;
use rotero_pdf::PageTextData;

use super::app_state::{PdfTabManager, RenderedPageData, TabId};

/// JPEG quality for the initial fast render pass (progressive rendering).
pub const PREVIEW_QUALITY: u8 = 25;

/// Result type for PDF text/metadata extraction.
pub type PdfExtractResult = (Vec<(u32, String)>, rotero_pdf::PdfDocMetadata);

/// A request to the PDF render thread.
pub enum RenderRequest {
    OpenPdf {
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
        quality: u8,
        reply: mpsc::Sender<Result<(u32, Vec<RenderedPageData>), String>>,
    },
    RenderMorePages {
        pdf_path: String,
        start: u32,
        count: u32,
        zoom: f32,
        quality: u8,
        reply: mpsc::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    SetZoom {
        pdf_path: String,
        page_count: u32,
        new_zoom: f32,
        quality: u8,
        reply: mpsc::Sender<Result<Vec<RenderedPageData>, String>>,
    },
    ExtractText {
        pdf_path: String,
        /// (page_index, img_width, img_height) for each page to extract
        page_dims: Vec<(u32, u32, u32)>,
        reply: mpsc::Sender<Result<HashMap<u32, PageTextData>, String>>,
    },
    RenderThumbnails {
        pdf_path: String,
        start: u32,
        count: u32,
        quality: u8,
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
    /// Extract raw text from first N pages + PDF document properties for metadata extraction.
    ExtractMetadataText {
        pdf_path: String,
        page_count: u32,
        reply: mpsc::Sender<Result<PdfExtractResult, String>>,
    },
    /// Extract existing annotations from the PDF file.
    ExtractAnnotations {
        pdf_path: String,
        reply: mpsc::Sender<Result<Vec<rotero_pdf::ExtractedAnnotation>, String>>,
    },
    /// Re-render specific pages at higher quality (progressive rendering upgrade pass).
    UpgradeQuality {
        pdf_path: String,
        page_indices: Vec<u32>,
        zoom: f32,
        quality: u8,
        reply: mpsc::Sender<Result<Vec<RenderedPageData>, String>>,
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
                    quality,
                    reply,
                } => {
                    let result = (|| {
                        let info = engine.load_document(&pdf_path).map_err(|e| e.to_string())?;
                        let render_count = info.page_count.min(batch_size);
                        let rendered = engine
                            .render_pages(&pdf_path, 0, render_count, zoom, quality)
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
                    quality,
                    reply,
                } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, start, count, zoom, quality)
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
                    quality,
                    reply,
                } => {
                    let result = (|| {
                        let rendered = engine
                            .render_pages(&pdf_path, 0, page_count, new_zoom, quality)
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
                    quality,
                    reply,
                } => {
                    let result = (|| {
                        let rendered = engine
                            .render_thumbnails_range(&pdf_path, start, count, 120, quality)
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
                RenderRequest::UpgradeQuality {
                    pdf_path,
                    page_indices,
                    zoom,
                    quality,
                    reply,
                } => {
                    let result = (|| {
                        let mut pages = Vec::with_capacity(page_indices.len());
                        for &idx in &page_indices {
                            match engine.render_page(&pdf_path, idx, zoom, quality) {
                                Ok(rp) => pages.push(rp.into()),
                                Err(e) => {
                                    tracing::warn!(page = idx, "upgrade render failed: {e}");
                                }
                            }
                        }
                        Ok(pages)
                    })();
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

/// Re-render pages at higher quality (background upgrade pass for progressive rendering).
/// Only upgrades pages that are still resident and at lower quality than `quality`.
/// Skips entirely if the tab's zoom has changed since the request was initiated.
pub async fn upgrade_quality(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    page_indices: Vec<u32>,
    quality: u8,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    let (pdf_path, zoom, dpr, render_zoom) = {
        let mgr = tabs.read();
        let tab = mgr
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.view.zoom, tab.view.dpr, tab.view.render_zoom)
    };

    let render_scale = zoom * dpr;

    // Skip if zoom changed (pages will be re-rendered at new zoom anyway)
    if (render_zoom - render_scale).abs() > 0.01 {
        return Ok(());
    }

    // Filter to only pages still resident with lower quality
    let indices_to_upgrade: Vec<u32> = {
        let mgr = tabs.read();
        let Some(tab) = mgr.tabs.iter().find(|t| t.id == tab_id) else {
            return Ok(());
        };
        page_indices
            .into_iter()
            .filter(|&idx| {
                tab.render
                    .rendered_pages
                    .iter()
                    .find(|p| p.page_index == idx)
                    .map_or(false, |p| p.quality < quality)
            })
            .collect()
    };

    if indices_to_upgrade.is_empty() {
        return Ok(());
    }

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::UpgradeQuality {
            pdf_path: pdf_path.clone(),
            page_indices: indices_to_upgrade,
            zoom: render_scale,
            quality,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let pages = recv_reply(reply_rx).await?;

    // Re-check zoom hasn't changed while we were rendering
    let current_render_scale = tabs
        .read()
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .map(|t| t.view.zoom * t.view.dpr)
        .unwrap_or(0.0);
    if (current_render_scale - render_scale).abs() > 0.01 {
        return Ok(());
    }

    // Insert only if quality is better than what's currently there
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            for p in &pages {
                tab.render.insert_if_better(p.clone());
            }
        }
    });

    // Save upgraded pages to disk cache in background
    let page_count = tabs
        .read()
        .tabs
        .iter()
        .find(|t| t.id == tab_id)
        .map(|t| t.page_count)
        .unwrap_or(0);
    let cache_dir = data_dir.to_path_buf();
    let cache_path = pdf_path;
    std::thread::spawn(move || {
        crate::cache::save_pages(&cache_dir, &cache_path, render_scale, page_count, &pages);
    });

    Ok(())
}

/// Open/render a PDF tab's first batch of pages.
/// Uses disk cache when available for instant loading.
pub async fn open_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    data_dir: &std::path::Path,
    quality: u8,
) -> Result<(), String> {
    let (path, zoom, dpr, batch_size) = {
        let mgr = tabs.read();
        let tab = mgr
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?;
        (
            tab.pdf_path.clone(),
            tab.view.zoom,
            tab.view.dpr,
            tab.view.page_batch_size,
        )
    };

    let render_scale = zoom * dpr;

    // Try loading from disk cache first
    if let Some((meta, cached_pages)) = crate::cache::load_cached(data_dir, &path, render_scale) {
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.page_count = meta.page_count;
                tab.view.render_zoom = render_scale;
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

    // Cache miss — render preview at low quality first, then upgrade in background
    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::OpenPdf {
            pdf_path: path.clone(),
            zoom: render_scale,
            batch_size,
            quality: PREVIEW_QUALITY,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let (page_count, pages) = recv_reply(reply_rx).await?;

    let preview_count = pages.len() as u32;
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.page_count = page_count;
            tab.view.render_zoom = render_scale;
            tab.render.rendered_pages = pages;
            tab.is_loading = false;
        }
    });

    // Spawn background quality upgrade
    let upgrade_indices: Vec<u32> = (0..preview_count).collect();
    let render_tx_up = render_tx.clone();
    let mut tabs_up = *tabs;
    let data_dir_up = data_dir.to_path_buf();
    spawn(async move {
        let _ = upgrade_quality(
            &render_tx_up,
            &mut tabs_up,
            tab_id,
            upgrade_indices,
            quality,
            &data_dir_up,
        )
        .await;
    });

    // Extract text in background — don't block the render thread
    let page_dims: Vec<(u32, u32, u32)> = {
        let mgr = tabs.read();
        mgr.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| {
                t.render
                    .rendered_pages
                    .iter()
                    .map(|p| (p.page_index, p.width, p.height))
                    .collect()
            })
            .unwrap_or_default()
    };
    let render_tx2 = render_tx.clone();
    let data_dir2 = data_dir.to_path_buf();
    let path2 = path.clone();
    let mut tabs2 = *tabs;
    spawn(async move {
        let (text_tx, text_rx) = mpsc::channel();
        let _ = render_tx2.send(RenderRequest::ExtractText {
            pdf_path: path2.clone(),
            page_dims,
            reply: text_tx,
        });
        if let Ok(text_data) = recv_reply(text_rx).await {
            let cache_dir = data_dir2.clone();
            let cache_path = path2.clone();
            let text_clone = text_data.clone();
            std::thread::spawn(move || {
                crate::cache::save_text(&cache_dir, &cache_path, &text_clone);
            });

            tabs2.with_mut(|mgr| {
                if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.render.text_data = text_data;
                }
            });
        }
    });

    Ok(())
}

/// Render additional pages for lazy loading on scroll.
pub async fn render_more_pages(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    start: u32,
    count: u32,
    quality: u8,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    let (pdf_path, zoom, dpr) = {
        let mgr = tabs.read();
        let tab = mgr
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.view.zoom, tab.view.dpr)
    };

    let render_scale = zoom * dpr;

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderMorePages {
            pdf_path: pdf_path.clone(),
            start,
            count,
            zoom: render_scale,
            quality: PREVIEW_QUALITY,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let pages = recv_reply(reply_rx).await?;

    // Collect dims before mutating — we need them for text extraction
    let page_dims: Vec<(u32, u32, u32)> = pages
        .iter()
        .map(|p| (p.page_index, p.width, p.height))
        .collect();

    let upgrade_indices: Vec<u32> = pages.iter().map(|p| p.page_index).collect();

    // Save new pages to disk cache in background
    let cache_pages = pages.clone();
    let cache_dir = data_dir.to_path_buf();
    let cache_path = pdf_path.clone();
    let page_count = tabs.read().active_tab().map(|t| t.page_count).unwrap_or(0);
    std::thread::spawn(move || {
        crate::cache::save_pages(&cache_dir, &cache_path, render_scale, page_count, &cache_pages);
    });

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.render.rendered_pages.extend(pages);
        }
    });

    // Spawn background quality upgrade
    let render_tx_up = render_tx.clone();
    let mut tabs_up = *tabs;
    let data_dir_up = data_dir.to_path_buf();
    spawn(async move {
        let _ = upgrade_quality(
            &render_tx_up,
            &mut tabs_up,
            tab_id,
            upgrade_indices,
            quality,
            &data_dir_up,
        )
        .await;
    });

    let render_tx2 = render_tx.clone();
    let mut tabs2 = *tabs;
    spawn(async move {
        let (text_tx, text_rx) = mpsc::channel();
        let _ = render_tx2.send(RenderRequest::ExtractText {
            pdf_path,
            page_dims,
            reply: text_tx,
        });
        if let Ok(text_data) = recv_reply(text_rx).await {
            tabs2.with_mut(|mgr| {
                if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.render.text_data.extend(text_data);
                }
            });
        }
    });

    Ok(())
}

/// Change zoom level and re-render pages around the current viewport.
/// Only re-renders up to MAX_RESIDENT_PAGES pages instead of all loaded pages,
/// and evicts distant pages afterward to bound peak memory during zoom.
pub async fn set_zoom(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    new_zoom: f32,
    quality: u8,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    let (pdf_path, page_count, dpr) = {
        let mgr = tabs.read();
        let tab = mgr
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.rendered_count(), tab.view.dpr)
    };

    let render_scale = new_zoom * dpr;

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
            new_zoom: render_scale,
            quality: PREVIEW_QUALITY,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let pages = recv_reply(reply_rx).await?;

    let upgrade_indices: Vec<u32> = pages.iter().map(|p| p.page_index).collect();

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.view.render_zoom = render_scale;
            tab.render.rendered_pages = pages;
            tab.render.text_data.clear(); // will be re-extracted at new zoom
        }
    });

    // Spawn background quality upgrade
    let render_tx_up = render_tx.clone();
    let mut tabs_up = *tabs;
    let data_dir_up = data_dir.to_path_buf();
    spawn(async move {
        let _ = upgrade_quality(
            &render_tx_up,
            &mut tabs_up,
            tab_id,
            upgrade_indices,
            quality,
            &data_dir_up,
        )
        .await;
    });

    Ok(())
}

/// Maximum number of thumbnails kept in memory at once.
const MAX_RESIDENT_THUMBS: usize = 50;

/// Load a batch of thumbnails starting from `start`.
pub async fn load_thumbnails(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    start: u32,
    count: u32,
    quality: u8,
) -> Result<(), String> {
    let pdf_path = {
        let mgr = tabs.read();
        mgr.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?
            .pdf_path
            .clone()
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderThumbnails {
            pdf_path,
            start,
            count,
            quality,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let thumbnails = recv_reply(reply_rx).await?;

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            for thumb in thumbnails {
                tab.render.thumbnails.insert(thumb.page_index, thumb);
            }
            // Evict distant thumbnails to bound memory
            let center = start + count / 2;
            if tab.render.thumbnails.len() > MAX_RESIDENT_THUMBS {
                let half = MAX_RESIDENT_THUMBS as u32 / 2;
                let lo = center.saturating_sub(half);
                let hi = center.saturating_add(half);
                tab.render
                    .thumbnails
                    .retain(|&idx, _| idx >= lo && idx <= hi);
            }
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
        mgr.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?
            .pdf_path
            .clone()
    };

    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::ExtractOutline {
            pdf_path,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;

    let outline = recv_reply(reply_rx).await?;

    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.nav.outline = outline;
        }
    });

    Ok(())
}

/// Pre-cache a PDF in the background (render pages + extract text to disk + index fulltext).
/// Fire-and-forget — does not update any UI state.
pub async fn precache_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    pdf_path: &str,
    data_dir: &std::path::Path,
    zoom: f32,
    quality: u8,
    paper_id: Option<i64>,
    db: Option<&turso::Connection>,
) {
    // Skip if already cached
    if crate::cache::load_cached(data_dir, pdf_path, zoom).is_some() {
        return;
    }

    let path = pdf_path.to_string();

    // Render first batch of pages
    let (reply_tx, reply_rx) = mpsc::channel();
    if render_tx
        .send(RenderRequest::OpenPdf {
            pdf_path: path.clone(),
            zoom,
            batch_size: 5,
            quality,
            reply: reply_tx,
        })
        .is_err()
    {
        return;
    }

    let Ok((page_count, pages)) = recv_reply(reply_rx).await else {
        return;
    };

    // Save pages to cache
    let dir = data_dir.to_path_buf();
    let p = path.clone();
    let pg = pages.clone();
    std::thread::spawn(move || {
        crate::cache::save_pages(&dir, &p, zoom, page_count, &pg);
    });

    // Extract and cache text using actual rendered dims
    let page_dims: Vec<(u32, u32, u32)> = pages
        .iter()
        .map(|p| (p.page_index, p.width, p.height))
        .collect();
    let (text_tx, text_rx) = mpsc::channel();
    if render_tx
        .send(RenderRequest::ExtractText {
            pdf_path: path.clone(),
            page_dims,
            reply: text_tx,
        })
        .is_err()
    {
        return;
    }

    if let Ok(text_data) = recv_reply(text_rx).await {
        // Concatenate all text segments for full-text search
        if let (Some(pid), Some(conn)) = (paper_id, db) {
            let fulltext: String = text_data
                .values()
                .flat_map(|td| td.segments.iter().map(|s| s.text.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            if !fulltext.is_empty() {
                let _ = crate::db::papers::update_paper_fulltext(conn, pid, &fulltext).await;
            }
        }

        let dir = data_dir.to_path_buf();
        std::thread::spawn(move || {
            crate::cache::save_text(&dir, &path, &text_data);
        });
    }
}

/// Extract metadata from a PDF and update the paper record.
///
/// The paper is already inserted with filename as title. This function:
/// 1. Extracts raw text from the first 2 pages (via render thread)
/// 2. Reads PDF document properties (title, author)
/// 3. Tries to find a DOI in the text
/// 4. If DOI found + auto_fetch, calls CrossRef for full metadata
/// 5. Falls back to PDF document properties if CrossRef unavailable
pub async fn extract_and_fetch_metadata(
    render_tx: &mpsc::Sender<RenderRequest>,
    conn: &turso::Connection,
    paper_id: i64,
    pdf_path: &str,
    auto_fetch: bool,
    lib_state: &mut Signal<super::app_state::LibraryState>,
) {
    tracing::info!(
        paper_id,
        pdf_path,
        auto_fetch,
        "extract_and_fetch_metadata: start"
    );

    // 1. Extract raw text + doc properties via render thread
    let (reply_tx, reply_rx) = mpsc::channel();
    if render_tx
        .send(RenderRequest::ExtractMetadataText {
            pdf_path: pdf_path.to_string(),
            page_count: 2,
            reply: reply_tx,
        })
        .is_err()
    {
        tracing::warn!("extract_and_fetch_metadata: failed to send render request");
        return;
    }

    let Ok((raw_pages, doc_meta)) = recv_reply(reply_rx).await else {
        tracing::warn!("extract_and_fetch_metadata: render thread reply failed");
        return;
    };

    tracing::info!(
        pages = raw_pages.len(),
        total_chars = raw_pages.iter().map(|(_, t)| t.len()).sum::<usize>(),
        doc_title = ?doc_meta.title,
        doc_author = ?doc_meta.author,
        "extract_and_fetch_metadata: text extracted"
    );

    // 2. Try to extract DOI and arXiv ID from first 2 pages
    let combined_text: String = raw_pages
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let doi = crate::metadata::doi_extract::extract_doi(&combined_text);
    let arxiv_id = crate::metadata::doi_extract::extract_arxiv_id(&combined_text);
    tracing::info!(?doi, ?arxiv_id, "extract_and_fetch_metadata: ID extraction");

    // 3. If DOI found and auto_fetch enabled, call CrossRef
    if let Some(ref doi_str) = doi
        && auto_fetch
    {
        match crate::metadata::crossref::fetch_by_doi(doi_str).await {
            Ok(meta) => {
                tracing::info!(title = %meta.title, authors = ?meta.authors, "extract_and_fetch_metadata: CrossRef success");
                let fetched = crate::metadata::parser::metadata_to_paper(meta);
                if apply_fetched_metadata(conn, paper_id, &fetched, lib_state).await {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(%e, "extract_and_fetch_metadata: CrossRef lookup failed");
            }
        }
    }

    // 3b. If arXiv ID found and auto_fetch enabled, call arXiv API
    if let Some(ref arxiv) = arxiv_id
        && auto_fetch
    {
        match crate::metadata::arxiv::fetch_by_arxiv_id(arxiv).await {
            Ok(meta) => {
                tracing::info!(title = %meta.title, authors = ?meta.authors, "extract_and_fetch_metadata: arXiv success");
                let fetched = crate::metadata::parser::metadata_to_paper(meta);
                if apply_fetched_metadata(conn, paper_id, &fetched, lib_state).await {
                    return;
                }
            }
            Err(e) => {
                tracing::warn!(%e, "extract_and_fetch_metadata: arXiv lookup failed");
            }
        }
    }

    // 4. Fallback: use PDF document properties + extracted DOI/arXiv ID
    let has_update = doc_meta.title.is_some()
        || doc_meta.author.is_some()
        || doi.is_some()
        || arxiv_id.is_some();
    if !has_update {
        tracing::info!("extract_and_fetch_metadata: no metadata found");
        return;
    }

    lib_state.with_mut(|s| {
        if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
            if let Some(ref title) = doc_meta.title {
                p.title = title.clone();
            }
            if let Some(ref author) = doc_meta.author {
                p.authors = author
                    .split(';')
                    .flat_map(|s| s.split(','))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            if let Some(ref doi_str) = doi {
                p.doi = Some(doi_str.clone());
            } else if let Some(ref arxiv) = arxiv_id {
                p.doi = Some(format!("arXiv:{arxiv}"));
            }
        }
    });

    // Persist fallback metadata to DB
    let paper_snapshot = lib_state
        .read()
        .papers
        .iter()
        .find(|p| p.id == Some(paper_id))
        .cloned();
    if let Some(paper) = paper_snapshot {
        let _ = crate::db::papers::update_paper_metadata(conn, paper_id, &paper).await;
    }
}

/// Apply fetched metadata to DB and in-memory state. Returns true on success.
async fn apply_fetched_metadata(
    conn: &turso::Connection,
    paper_id: i64,
    fetched: &rotero_models::Paper,
    lib_state: &mut Signal<super::app_state::LibraryState>,
) -> bool {
    if crate::db::papers::update_paper_metadata(conn, paper_id, fetched)
        .await
        .is_err()
    {
        return false;
    }
    lib_state.with_mut(|s| {
        if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
            p.title = fetched.title.clone();
            p.authors = fetched.authors.clone();
            p.year = fetched.year;
            p.doi = fetched.doi.clone();
            p.abstract_text = fetched.abstract_text.clone();
            p.journal = fetched.journal.clone();
            p.volume = fetched.volume.clone();
            p.issue = fetched.issue.clone();
            p.pages = fetched.pages.clone();
            p.publisher = fetched.publisher.clone();
            p.url = fetched.url.clone();
            if fetched.citation_count.is_some() {
                p.citation_count = fetched.citation_count;
            }
        }
    });
    // Persist citation count separately (update_paper_metadata doesn't include it)
    if let Some(count) = fetched.citation_count {
        let _ = crate::db::papers::update_citation_count(conn, paper_id, count).await;
    }
    true
}

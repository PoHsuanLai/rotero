use std::sync::mpsc;

use dioxus::prelude::*;

use super::{recv_reply, RenderRequest};
use crate::state::app_state::{PdfTabManager, TabId};

pub async fn open_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    let (path, zoom, dpr, batch_size, paper_id) = {
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
            tab.paper_id.clone(),
        )
    };
    let render_scale = zoom * dpr;
    let cache_dir = data_dir.to_path_buf();
    let cache_path = path.clone();
    type CacheResult = (
        Option<(crate::cache::CacheMeta, Vec<crate::state::app_state::RenderedPageData>)>,
        Option<std::collections::HashMap<u32, rotero_pdf::PageTextData>>,
    );
    let (cache_tx, cache_rx) = mpsc::channel::<CacheResult>();
    std::thread::spawn(move || {
        let result = crate::cache::load_cached(&cache_dir, &cache_path, render_scale);
        let text = crate::cache::load_cached_text(&cache_dir, &cache_path);
        let _ = cache_tx.send((result, text));
    });
    let cache_result = tokio::task::spawn_blocking(move || cache_rx.recv())
        .await
        .ok()
        .and_then(|r| r.ok());
    if let Some((Some((meta, cached_pages)), text_data)) = cache_result {
        let cached_count = cached_pages.len() as u32;
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.page_count = meta.page_count;
                tab.view.render_zoom = render_scale;
                tab.render.rendered_pages = cached_pages;
                tab.is_loading = false;
            }
        });
        if let Some(text_data) = text_data {
            tabs.with_mut(|mgr| {
                if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.render.text_data = text_data;
                }
            });
        }
        if cached_count < meta.page_count {
            let render_tx_bg = render_tx.clone();
            let data_dir_bg = data_dir.to_path_buf();
            let mut tabs_bg = *tabs;
            let total = meta.page_count;
            spawn(async move {
                let rendered_indices: std::collections::HashSet<u32> = tabs_bg
                    .read()
                    .tabs
                    .iter()
                    .find(|t| t.id == tab_id)
                    .map(|t| t.render.rendered_pages.iter().map(|p| p.page_index).collect())
                    .unwrap_or_default();
                let mut missing: Vec<u32> = (0..total).filter(|i| !rendered_indices.contains(i)).collect();
                missing.sort();
                for chunk in missing.chunks(batch_size as usize) {
                    let start = chunk[0];
                    let count = (chunk.last().unwrap() - start + 1) as u32;
                    if render_more_pages(
                        &render_tx_bg, &mut tabs_bg, tab_id, start, count, &data_dir_bg,
                    ).await.is_err() {
                        break;
                    }
                }
            });
        }
        return Ok(());
    }
    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::OpenPdf {
            pdf_path: path.clone(),
            zoom: render_scale,
            batch_size,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;
    let (page_count, pages) = recv_reply(reply_rx).await?;
    let cache_pages = pages.clone();
    let cache_dir = data_dir.to_path_buf();
    let cache_path = path.clone();
    std::thread::spawn(move || {
        crate::cache::save_pages(
            &cache_dir,
            &cache_path,
            render_scale,
            page_count,
            &cache_pages,
        );
    });
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.page_count = page_count;
            tab.view.render_zoom = render_scale;
            tab.render.rendered_pages = pages;
            tab.is_loading = false;
        }
    });
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
    let paper_id2 = paper_id.clone();
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

            if let Some(ref pid) = paper_id2 {
                let fulltext: String = text_data
                    .values()
                    .flat_map(|td| td.segments.iter().map(|s| s.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                if !fulltext.is_empty() {
                    #[cfg(feature = "desktop")]
                    if let Some((conn, _)) = crate::init::database::SHARED_DB.get() {
                        let pid = pid.clone();
                        let conn = conn.clone();
                        spawn(async move {
                            let _ = rotero_db::papers::update_paper_fulltext(&conn, &pid, &fulltext).await;
                        });
                    }
                }
            }

            tabs2.with_mut(|mgr| {
                if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.render.text_data = text_data;
                }
            });
        }
    });

    let rendered_so_far = batch_size.min(page_count);
    if rendered_so_far < page_count {
        let render_tx_bg = render_tx.clone();
        let data_dir_bg = data_dir.to_path_buf();
        let mut tabs_bg = *tabs;
        spawn(async move {
            let mut start = rendered_so_far;
            while start < page_count {
                let count = batch_size.min(page_count - start);
                if render_more_pages(
                    &render_tx_bg, &mut tabs_bg, tab_id, start, count, &data_dir_bg,
                ).await.is_err() {
                    break;
                }
                start += count;
            }
        });
    }

    Ok(())
}

pub async fn render_more_pages(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    start: u32,
    count: u32,
    data_dir: &std::path::Path,
) -> Result<(), String> {
    let (pdf_path, render_scale) = {
        let mgr = tabs.read();
        let tab = mgr
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?;
        (tab.pdf_path.clone(), tab.view.render_zoom)
    };
    let (reply_tx, reply_rx) = mpsc::channel();
    render_tx
        .send(RenderRequest::RenderMorePages {
            pdf_path: pdf_path.clone(),
            start,
            count,
            zoom: render_scale,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;
    let pages = recv_reply(reply_rx).await?;
    let page_dims: Vec<(u32, u32, u32)> = pages
        .iter()
        .map(|p| (p.page_index, p.width, p.height))
        .collect();
    let cache_pages = pages.clone();
    let cache_dir = data_dir.to_path_buf();
    let cache_path = pdf_path.clone();
    let page_count = tabs.read().active_tab().map(|t| t.page_count).unwrap_or(0);
    std::thread::spawn(move || {
        crate::cache::save_pages(
            &cache_dir,
            &cache_path,
            render_scale,
            page_count,
            &cache_pages,
        );
    });
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.render.rendered_pages.extend(pages);
        }
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

pub fn set_zoom(
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    new_zoom: f32,
) {
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.view.zoom = new_zoom;
        }
    });
}

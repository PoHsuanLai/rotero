use std::sync::mpsc;

use dioxus::prelude::*;
use rotero_pdf::RenderFormat;

use super::{recv_reply, RenderRequest};
use crate::state::app_state::{PdfTabManager, TabId};

pub async fn open_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    data_dir: &std::path::Path,
    quality: u8,
    format: RenderFormat,
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
    if let Some((meta, cached_pages)) = crate::cache::load_cached(data_dir, &path, render_scale) {
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.page_count = meta.page_count;
                tab.view.render_zoom = render_scale;
                tab.render.rendered_pages = cached_pages;
                tab.is_loading = false;
            }
        });
        if let Some(text_data) = crate::cache::load_cached_text(data_dir, &path) {
            tabs.with_mut(|mgr| {
                if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                    tab.render.text_data = text_data;
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
            quality,
            format,
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

pub async fn render_more_pages(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    start: u32,
    count: u32,
    quality: u8,
    format: RenderFormat,
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
            quality,
            format,
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

pub async fn set_zoom(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    new_zoom: f32,
    quality: u8,
    format: RenderFormat,
    _data_dir: &std::path::Path,
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
            quality,
            format,
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;
    let pages = recv_reply(reply_rx).await?;
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.view.render_zoom = render_scale;
            tab.render.rendered_pages = pages;
            tab.render.text_data.clear();
        }
    });
    Ok(())
}

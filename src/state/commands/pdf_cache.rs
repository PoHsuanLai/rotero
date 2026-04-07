use std::sync::mpsc;

use dioxus::prelude::*;
use super::{recv_reply, RenderRequest};
use crate::state::app_state::{PdfTabManager, TabId};

const MAX_RESIDENT_THUMBS: usize = 50;

pub async fn load_thumbnails(
    render_tx: &mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    start: u32,
    count: u32,
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
            reply: reply_tx,
        })
        .map_err(|e| e.to_string())?;
    let thumbnails = recv_reply(reply_rx).await?;
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            for thumb in thumbnails {
                tab.render.thumbnails.insert(thumb.page_index, thumb);
            }
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

pub async fn precache_pdf(
    render_tx: &mpsc::Sender<RenderRequest>,
    pdf_path: &str,
    data_dir: &std::path::Path,
    zoom: f32,
    paper_id: Option<String>,
    db: Option<&rotero_db::turso::Connection>,
) {
    if crate::cache::load_cached(data_dir, pdf_path, zoom).is_some() {
        return;
    }
    let path = pdf_path.to_string();
    let (reply_tx, reply_rx) = mpsc::channel();
    if render_tx
        .send(RenderRequest::OpenPdf {
            pdf_path: path.clone(),
            zoom,
            batch_size: 5,
            reply: reply_tx,
        })
        .is_err()
    {
        return;
    }
    let Ok((page_count, pages)) = recv_reply(reply_rx).await else {
        return;
    };
    let dir = data_dir.to_path_buf();
    let p = path.clone();
    let pg = pages.clone();
    std::thread::spawn(move || {
        crate::cache::save_pages(&dir, &p, zoom, page_count, &pg);
    });
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
        if let (Some(pid), Some(conn)) = (&paper_id, db) {
            let fulltext: String = text_data
                .values()
                .flat_map(|td| td.segments.iter().map(|s| s.text.as_str()))
                .collect::<Vec<_>>()
                .join("");
            if !fulltext.is_empty() {
                let _ = rotero_db::papers::update_paper_fulltext(conn, pid, &fulltext).await;
            }
        }
        let dir = data_dir.to_path_buf();
        std::thread::spawn(move || {
            crate::cache::save_text(&dir, &path, &text_data);
        });
    }
}

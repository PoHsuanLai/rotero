use super::PdfDocs;
use crate::state::app_state::{PdfTabManager, TabId};
use dioxus::prelude::*;

const MAX_RESIDENT_THUMBS: usize = 50;

pub async fn load_thumbnails(
    docs: &PdfDocs,
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
    let thumbnails = docs.render_thumbnails(pdf_path, start, count).await?;
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
    docs: &PdfDocs,
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
    let outline = docs.extract_outline(pdf_path).await?;
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.nav.outline = outline;
        }
    });
    Ok(())
}

/// Extracts intra-document links (citation/figure/section jumps) once and stores
/// them on the tab, keyed by source page for the overlay.
pub async fn load_links(
    docs: &PdfDocs,
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
    let links = docs.extract_links(pdf_path).await?;
    let by_page = crate::state::app_state::build_page_links(&links);
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.links = by_page;
        }
    });
    Ok(())
}

pub async fn precache_pdf(
    docs: &PdfDocs,
    pdf_path: &str,
    data_dir: &std::path::Path,
    zoom: f32,
    paper_id: Option<String>,
    db: Option<&rotero_db::Database>,
) {
    if crate::cache::load_cached(data_dir, pdf_path, zoom).is_some() {
        return;
    }
    let path = pdf_path.to_string();
    let Ok((page_count, pages)) = docs
        .open_and_render_initial(path.clone(), zoom, 5)
        .await
    else {
        return;
    };
    let batch_size = 5u32;
    let dir = data_dir.to_path_buf();
    let p = path.clone();
    let pg = pages.clone();
    std::thread::spawn(move || {
        crate::cache::save_pages(&dir, &p, zoom, page_count, &pg);
    });

    // Collect all page dims: start with initial batch, then render remaining
    let mut all_page_dims: Vec<(u32, u32, u32)> = pages
        .iter()
        .map(|p| (p.page_index, p.width, p.height))
        .collect();

    let mut start = batch_size.min(page_count);
    while start < page_count {
        let count = batch_size.min(page_count - start);
        let Ok(more_pages) = docs
            .render_pages(path.clone(), start, count, zoom)
            .await
        else {
            break;
        };
        let more_dir = data_dir.to_path_buf();
        let more_path = path.clone();
        let more_pg = more_pages.clone();
        std::thread::spawn(move || {
            crate::cache::save_pages(&more_dir, &more_path, zoom, page_count, &more_pg);
        });
        all_page_dims.extend(more_pages.iter().map(|p| (p.page_index, p.width, p.height)));
        start += count;
    }

    let Ok(text_data) = docs.extract_text(path.clone(), all_page_dims).await else {
        return;
    };
    if let (Some(pid), Some(db)) = (&paper_id, db) {
        let mut pages_sorted: Vec<u32> = text_data.keys().copied().collect();
        pages_sorted.sort();
        let fulltext: String = pages_sorted
            .iter()
            .filter_map(|p| text_data.get(p))
            .flat_map(|td| td.segments.iter().map(|s| s.text.as_str()))
            .collect::<Vec<_>>()
            .join("");
        if !fulltext.is_empty() {
            let _ = db.update_paper_fulltext(pid, &fulltext).await;
        }
    }
    let dir = data_dir.to_path_buf();
    std::thread::spawn(move || {
        crate::cache::save_text(&dir, &path, &text_data);
    });
}

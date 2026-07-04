use tokio::sync::oneshot;

use dioxus::prelude::*;

use super::{RenderRequest, recv_reply};
use crate::state::app_state::{PdfTabManager, RenderedPageData, TabId};

/// Max full-resolution pages kept resident per tab. Full pages are far larger
/// than thumbnails (`MAX_RESIDENT_THUMBS = 50`), so this window is smaller.
/// Rendered pages outside a window centered on the current page are evicted.
pub const MAX_RESIDENT_PAGES: usize = 30;

/// Number of pages to render/keep on each side of the current page. The full
/// resident window is `2 * PAGE_WINDOW_RADIUS + 1` pages, which must stay
/// within `MAX_RESIDENT_PAGES`.
pub const PAGE_WINDOW_RADIUS: u32 = 12;

/// Evicts rendered pages far from `center` once the resident set exceeds
/// `MAX_RESIDENT_PAGES`, keeping a window `[center - radius, center + radius]`.
/// Mirrors the thumbnail windowing in `pdf_cache.rs`.
fn evict_pages_outside_window(
    rendered: &mut std::collections::BTreeMap<u32, RenderedPageData>,
    center: u32,
) {
    if rendered.len() <= MAX_RESIDENT_PAGES {
        return;
    }
    let lo = center.saturating_sub(PAGE_WINDOW_RADIUS);
    let hi = center.saturating_add(PAGE_WINDOW_RADIUS);
    rendered.retain(|&idx, _| idx >= lo && idx <= hi);
}

/// Collect all extracted text from a tab's text_data and save it to the DB fulltext column.
#[cfg(feature = "desktop")]
async fn save_fulltext_to_db(tabs: &Signal<PdfTabManager>, tab_id: TabId, paper_id: &str) {
    // Small delay to let the last text extraction spawn finish
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let fulltext: String = {
        let mgr = tabs.read();
        mgr.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| {
                let mut pages: Vec<u32> = t.render.text_data.keys().copied().collect();
                pages.sort();
                pages
                    .iter()
                    .filter_map(|p| t.render.text_data.get(p))
                    .flat_map(|td| td.segments.iter().map(|s| s.text.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default()
    };
    if !fulltext.is_empty()
        && let Some((conn, _)) = crate::init::database::SHARED_DB.get()
    {
        let _ = rotero_db::papers::update_paper_fulltext(conn, paper_id, &fulltext).await;
    }
}

pub async fn open_pdf(
    render_tx: &std::sync::mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    data_dir: &std::path::Path,
    dpr: f32,
) -> Result<(), String> {
    let (path, zoom, batch_size, paper_id) = {
        let mgr = tabs.read();
        let tab = mgr
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or("Tab not found")?;
        (
            tab.pdf_path.clone(),
            tab.view.zoom,
            tab.view.page_batch_size,
            tab.paper_id.clone(),
        )
    };
    let render_scale = zoom * dpr;
    let cache_dir = data_dir.to_path_buf();
    let cache_path = path.clone();
    type CacheResult = (
        Option<(
            crate::cache::CacheMeta,
            Vec<crate::state::app_state::RenderedPageData>,
        )>,
        Option<std::collections::HashMap<u32, rotero_pdf::PageTextData>>,
    );
    let (cache_tx, cache_rx) = oneshot::channel::<CacheResult>();
    std::thread::spawn(move || {
        let result = crate::cache::load_cached(&cache_dir, &cache_path, render_scale);
        let text = crate::cache::load_cached_text(&cache_dir, &cache_path);
        let _ = cache_tx.send((result, text));
    });
    let cache_result = cache_rx.await.ok();
    if let Some((Some((meta, cached_pages)), text_data)) = cache_result {
        // How many pages the on-disk cache holds (decides whether it's complete),
        // independent of how many we keep resident in memory.
        let cached_count = cached_pages.len() as u32;
        // Only keep an initial window (around page 0) resident; the rest render
        // lazily on scroll even though they exist in the disk cache.
        let mut resident: std::collections::BTreeMap<u32, RenderedPageData> =
            cached_pages.into_iter().map(|p| (p.page_index, p)).collect();
        evict_pages_outside_window(&mut resident, 0);
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.page_count = meta.page_count;
                tab.view.dpr = dpr;
                tab.view.render_zoom = render_scale;
                tab.render.rendered_pages = resident;
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
        // Check if we need to render missing pages or re-extract text
        let text_page_count = tabs
            .read()
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.render.text_data.len() as u32)
            .unwrap_or(0);
        let need_render = cached_count < meta.page_count;
        let need_text = text_page_count < meta.page_count;

        if need_render || need_text {
            let render_tx_bg = render_tx.clone();
            let data_dir_bg = data_dir.to_path_buf();
            let mut tabs_bg = *tabs;
            let paper_id_bg = paper_id.clone();
            let path_bg = path.clone();
            spawn(async move {
                // Ensure the initial window (around page 0) is rendered. Remaining
                // pages render lazily on scroll rather than all up front.
                if need_render {
                    ensure_window_rendered(&render_tx_bg, &mut tabs_bg, tab_id, 0, &data_dir_bg)
                        .await;
                }
                // Text is needed for the WHOLE document (search + fulltext), extracted
                // without rendering images. Also refresh the on-disk text cache.
                if need_text {
                    extract_remaining_text(
                        &render_tx_bg,
                        &mut tabs_bg,
                        tab_id,
                        &path_bg,
                        render_scale,
                    )
                    .await;
                    let all_text: std::collections::HashMap<u32, rotero_pdf::PageTextData> = {
                        let mgr = tabs_bg.read();
                        mgr.tabs
                            .iter()
                            .find(|t| t.id == tab_id)
                            .map(|t| t.render.text_data.clone())
                            .unwrap_or_default()
                    };
                    if !all_text.is_empty() {
                        let dir = data_dir_bg.clone();
                        let path_save = path_bg.clone();
                        std::thread::spawn(move || {
                            crate::cache::save_text(&dir, &path_save, &all_text);
                        });
                    }
                }
                #[cfg(feature = "desktop")]
                if let Some(pid) = paper_id_bg {
                    save_fulltext_to_db(&tabs_bg, tab_id, &pid).await;
                }
            });
        }
        return Ok(());
    }
    let (reply_tx, reply_rx) = oneshot::channel();
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
            tab.view.dpr = dpr;
            tab.view.render_zoom = render_scale;
            tab.render.rendered_pages = pages.into_iter().map(|p| (p.page_index, p)).collect();
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
                    .values()
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
        let (text_tx, text_rx) = oneshot::channel();
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

    // Images are rendered lazily in a sliding window (see `ensure_window_rendered`),
    // so we no longer render every page here. But search and the DB fulltext column
    // need text for the WHOLE document, and text extraction only needs page pixel
    // dimensions (not rendered images). Extract text for all remaining pages using
    // dimensions computed from the point-size of each page, without rendering them.
    if batch_size < page_count {
        let render_tx_bg = render_tx.clone();
        let mut tabs_bg = *tabs;
        let paper_id_bg = paper_id.clone();
        let path_bg = path.clone();
        spawn(async move {
            extract_remaining_text(&render_tx_bg, &mut tabs_bg, tab_id, &path_bg, render_scale)
                .await;
            #[cfg(feature = "desktop")]
            if let Some(pid) = paper_id_bg {
                save_fulltext_to_db(&tabs_bg, tab_id, &pid).await;
            }
        });
    } else {
        // All pages fit in one batch — text already extracted above, save fulltext now.
        #[cfg(feature = "desktop")]
        if let Some(pid) = paper_id {
            let tabs_ref = *tabs;
            spawn(async move {
                save_fulltext_to_db(&tabs_ref, tab_id, &pid).await;
            });
        }
    }

    Ok(())
}

/// Fetches every page's point size via one cheap `GetPageDimensions` pass (no
/// rendering), converts to pixel dims at `render_scale`, stores them on the tab
/// (`render.page_dims`) for placeholder sizing, and returns them. Idempotent: if
/// dims are already populated, returns the cached copy without re-parsing.
pub async fn populate_page_dims(
    render_tx: &std::sync::mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    pdf_path: &str,
    render_scale: f32,
) -> Vec<(u32, u32)> {
    // Already populated for the full document? Reuse it.
    {
        let mgr = tabs.read();
        if let Some(t) = mgr.tabs.iter().find(|t| t.id == tab_id)
            && t.render.page_dims.len() as u32 == t.page_count
            && t.page_count > 0
        {
            return t.render.page_dims.clone();
        }
    }
    let (dim_tx, dim_rx) = oneshot::channel();
    if render_tx
        .send(RenderRequest::GetPageDimensions {
            pdf_path: pdf_path.to_string(),
            reply: dim_tx,
        })
        .is_err()
    {
        return Vec::new();
    }
    let Ok(point_dims) = recv_reply(dim_rx).await else {
        return Vec::new();
    };
    let pixel_dims: Vec<(u32, u32)> = point_dims
        .iter()
        .map(|(w_pts, h_pts)| {
            (
                (w_pts * render_scale).round().max(1.0) as u32,
                (h_pts * render_scale).round().max(1.0) as u32,
            )
        })
        .collect();
    tabs.with_mut(|mgr| {
        if let Some(t) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            t.render.page_dims = pixel_dims.clone();
        }
    });
    pixel_dims
}

/// Extracts text for every page not already in `text_data`, computing pixel
/// dimensions from each page's point size × `render_scale` (matching what a real
/// render would produce, so text-layer coordinates line up). Does NOT render page
/// images — keeps search/fulltext whole-document while images stay windowed.
async fn extract_remaining_text(
    render_tx: &std::sync::mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    pdf_path: &str,
    render_scale: f32,
) {
    // Populate per-page pixel dims (also used for placeholder sizing) and get them back.
    let pixel_dims = populate_page_dims(render_tx, tabs, tab_id, pdf_path, render_scale).await;
    if pixel_dims.is_empty() {
        return;
    }
    // Pages already having text (the initial rendered batch) are skipped.
    let have_text: std::collections::HashSet<u32> = {
        let mgr = tabs.read();
        mgr.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.render.text_data.keys().copied().collect())
            .unwrap_or_default()
    };
    let page_dims: Vec<(u32, u32, u32)> = pixel_dims
        .iter()
        .enumerate()
        .filter(|(i, _)| !have_text.contains(&(*i as u32)))
        .map(|(i, (w, h))| (i as u32, *w, *h))
        .collect();
    if page_dims.is_empty() {
        return;
    }
    let (text_tx, text_rx) = oneshot::channel();
    if render_tx
        .send(RenderRequest::ExtractText {
            pdf_path: pdf_path.to_string(),
            page_dims,
            reply: text_tx,
        })
        .is_err()
    {
        return;
    }
    if let Ok(text_data) = recv_reply(text_rx).await {
        tabs.with_mut(|mgr| {
            if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
                tab.render.text_data.extend(text_data);
            }
        });
    }
}

/// Ensures rendered page images cover the window `[center - radius, center + radius]`,
/// rendering any missing pages. Eviction of pages outside the window happens inside
/// `render_more_pages`. Called from the viewer's scroll handler as `center` changes.
pub async fn ensure_window_rendered(
    render_tx: &std::sync::mpsc::Sender<RenderRequest>,
    tabs: &mut Signal<PdfTabManager>,
    tab_id: TabId,
    center: u32,
    data_dir: &std::path::Path,
) {
    let (page_count, render_scale, dims_ready, cur_page, pdf_path, present): (
        u32,
        f32,
        bool,
        u32,
        String,
        std::collections::BTreeSet<u32>,
    ) = {
        let mgr = tabs.read();
        match mgr.tabs.iter().find(|t| t.id == tab_id) {
            Some(t) => (
                t.page_count,
                t.view.render_zoom,
                t.render.page_dims.len() as u32 == t.page_count && t.page_count > 0,
                t.view.current_page,
                t.pdf_path.clone(),
                t.render.rendered_pages.keys().copied().collect(),
            ),
            None => return,
        }
    };
    if page_count == 0 {
        return;
    }
    // Fast path: the scroll handler fires many times per second. If the center
    // page hasn't moved, dims are loaded, and the target window is already fully
    // resident, there's nothing to do — skip the state write so the viewer does
    // NOT re-render on every scroll tick.
    if dims_ready && center == cur_page {
        let lo = center.saturating_sub(PAGE_WINDOW_RADIUS);
        let hi = center.saturating_add(PAGE_WINDOW_RADIUS).min(page_count - 1);
        if (lo..=hi).all(|i| present.contains(&i)) {
            return;
        }
    }
    // Ensure placeholder dims exist for the whole document so unrendered pages
    // reserve scroll height — otherwise the container is only as tall as the few
    // rendered pages and scrolling can never reach (or trigger) the rest.
    if !dims_ready {
        let _ = populate_page_dims(render_tx, tabs, tab_id, &pdf_path, render_scale).await;
    }
    // Record the new center so eviction keeps the right window.
    tabs.with_mut(|mgr| {
        if let Some(t) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            t.view.current_page = center;
        }
    });
    let lo = center.saturating_sub(PAGE_WINDOW_RADIUS);
    let hi = center.saturating_add(PAGE_WINDOW_RADIUS).min(page_count - 1);
    // Render contiguous runs of missing pages in the window.
    let mut idx = lo;
    while idx <= hi {
        if present.contains(&idx) {
            idx += 1;
            continue;
        }
        let run_start = idx;
        while idx <= hi && !present.contains(&idx) {
            idx += 1;
        }
        let count = idx - run_start;
        if render_more_pages(render_tx, tabs, tab_id, run_start, count, data_dir)
            .await
            .is_err()
        {
            break;
        }
    }
}

pub async fn render_more_pages(
    render_tx: &std::sync::mpsc::Sender<RenderRequest>,
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
    let (reply_tx, reply_rx) = oneshot::channel();
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
            tab.render
                .rendered_pages
                .extend(pages.into_iter().map(|p| (p.page_index, p)));
            evict_pages_outside_window(&mut tab.render.rendered_pages, tab.view.current_page);
        }
    });
    let render_tx2 = render_tx.clone();
    let mut tabs2 = *tabs;
    spawn(async move {
        let (text_tx, text_rx) = oneshot::channel();
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

pub fn set_zoom(tabs: &mut Signal<PdfTabManager>, tab_id: TabId, new_zoom: f32) {
    tabs.with_mut(|mgr| {
        if let Some(tab) = mgr.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.view.zoom = new_zoom;
        }
    });
}

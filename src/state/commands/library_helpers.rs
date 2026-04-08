use dioxus::prelude::*;
use rotero_db::turso::Connection;
use rotero_db::Database;

use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::sync::engine::SyncConfig;

/// Reload all papers from DB into the library state signal.
pub async fn refresh_papers(conn: &Connection, lib_state: &mut Signal<LibraryState>) {
    if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
        lib_state.with_mut(|s| s.papers = papers);
    }
}

/// Reload duplicate groups from DB into the library state signal.
pub async fn refresh_duplicates(conn: &Connection, lib_state: &mut Signal<LibraryState>) {
    if let Ok(groups) = rotero_db::papers::find_duplicates(conn).await {
        lib_state.with_mut(|s| s.filter.duplicate_groups = Some(groups));
    }
}

/// Reload papers and clear duplicate groups, then re-detect duplicates.
/// Used after merge/delete operations in the duplicates view.
pub async fn refresh_papers_and_duplicates(
    conn: &Connection,
    lib_state: &mut Signal<LibraryState>,
) {
    if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
        lib_state.with_mut(|s| {
            s.papers = papers;
            s.filter.duplicate_groups = None;
        });
    }
    refresh_duplicates(conn, lib_state).await;
}

/// Open a PDF in the tab manager, switch to the viewer, and record the access time.
/// Consolidates the open-PDF sequence used across multiple UI components.
#[allow(clippy::too_many_arguments)]
pub fn open_paper_pdf(
    db: &Database,
    tabs: &mut Signal<PdfTabManager>,
    lib_state: &mut Signal<LibraryState>,
    config: &Signal<SyncConfig>,
    dpr_sig: &Signal<crate::app::DevicePixelRatio>,
    paper_id: &str,
    rel_path: &str,
    title: &str,
) {
    let full_path = db.resolve_pdf_path(rel_path);
    let path_str = full_path.to_string_lossy().to_string();
    let cfg = config.read();
    tabs.with_mut(|m| {
        m.open_or_switch(
            paper_id.to_string(),
            path_str,
            title.to_string(),
            cfg.pdf.default_zoom,
            cfg.pdf.page_batch_size,
            dpr_sig.read().0,
        )
    });
    let pid = paper_id.to_string();
    lib_state.with_mut(|s| {
        s.touch_paper(&pid);
        s.view = LibraryView::PdfViewer;
    });
    let db_touch = db.clone();
    spawn(async move {
        let _ = rotero_db::papers::touch_paper(db_touch.conn(), &pid).await;
    });
}

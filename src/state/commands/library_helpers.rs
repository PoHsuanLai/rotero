use dioxus::prelude::*;
use rotero_db::Database;

use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::sync::engine::SyncConfig;

/// Reload all papers from DB into the library state signal.
pub async fn refresh_papers(db: &Database, lib_state: &mut Signal<LibraryState>) {
    if let Ok(papers) = db.list_papers().await {
        lib_state.with_mut(|s| s.papers = papers);
    }
}

/// Reload duplicate groups from DB into the library state signal.
pub async fn refresh_duplicates(db: &Database, lib_state: &mut Signal<LibraryState>) {
    if let Ok(groups) = db.find_duplicates().await {
        lib_state.with_mut(|s| s.filter.duplicate_groups = Some(groups));
    }
}

/// Reload papers and clear duplicate groups, then re-detect duplicates.
/// Used after merge/delete operations in the duplicates view.
pub async fn refresh_papers_and_duplicates(db: &Database, lib_state: &mut Signal<LibraryState>) {
    if let Ok(papers) = db.list_papers().await {
        lib_state.with_mut(|s| {
            s.papers = papers;
            s.filter.duplicate_groups = None;
        });
    }
    refresh_duplicates(db, lib_state).await;
}

/// Apply favorite and/or read flags to `ids` in the DB and the in-memory list.
///
/// Single-item callers pass a toggled value; multi-item callers pass `true`.
/// `report` surfaces DB errors via [`LibraryState::report_error`]; keybindings
/// pass `false` and swallow them.
pub fn set_paper_flags(
    db: Database,
    mut lib_state: Signal<LibraryState>,
    ids: Vec<String>,
    favorite: Option<bool>,
    read: Option<bool>,
    report: bool,
) {
    spawn(async move {
        for pid in &ids {
            if let Some(fav) = favorite
                && let Err(e) = db.set_favorite(pid, fav).await
                && report
            {
                lib_state.with_mut(|s| {
                    s.report_error(format!("Could not update the favourite flag: {e}"))
                });
            }
            if let Some(is_read) = read
                && let Err(e) = db.set_read(pid, is_read).await
                && report
            {
                lib_state
                    .with_mut(|s| s.report_error(format!("Could not update the read flag: {e}")));
            }
        }
        lib_state.with_mut(|s| {
            for pid in &ids {
                if let Some(p) = s.paper_mut(pid) {
                    if let Some(fav) = favorite {
                        p.status.is_favorite = fav;
                    }
                    if let Some(is_read) = read {
                        p.status.is_read = is_read;
                    }
                }
            }
        });
    });
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
        let _ = db_touch.touch_paper(&pid).await;
    });
}

/// Import a dropped file if it is a PDF: copy into the library, insert a row,
/// precache the first pages, and optionally fetch metadata.
pub async fn import_dropped_pdf(
    db: &Database,
    docs: &crate::state::commands::PdfDocs,
    mut lib_state: Signal<LibraryState>,
    config: Signal<SyncConfig>,
    dpr: f32,
    path: &str,
    file_name: &str,
) {
    if !file_name.ends_with(".pdf") {
        return;
    }
    let title = std::path::Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    let (rel_path, sha256) = match db.import_pdf(path, Some(&title), None, None) {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!("Failed to import {file_name}: {e}");
            return;
        }
    };

    let mut paper = rotero_models::Paper {
        title,
        links: rotero_models::PaperLinks {
            pdf_path: Some(rel_path.clone()),
            ..Default::default()
        },
        ..Default::default()
    };
    let paper_id = match db.insert_paper(&paper).await {
        Ok(id) => {
            let _ = db.update_pdf_path(&id, &rel_path, Some(&sha256)).await;
            paper.id = Some(id.clone());
            lib_state.with_mut(|s| s.papers.insert(0, paper));
            Some(id)
        }
        Err(e) => {
            tracing::error!("Failed to insert paper: {e}");
            None
        }
    };

    let full_path = db.resolve_pdf_path(&rel_path).to_string_lossy().to_string();
    let cfg = config.read();
    let data_dir = cfg.effective_library_path();
    let zoom = cfg.pdf.default_zoom * dpr;
    let auto_fetch = cfg.auto_fetch_metadata;
    drop(cfg);

    let docs_pre = docs.clone();
    let db_for_cache = db.clone();
    let cache_path = full_path.clone();
    let pid_cache = paper_id.clone();
    spawn(async move {
        crate::state::commands::precache_pdf(
            &docs_pre,
            &cache_path,
            &data_dir,
            zoom,
            pid_cache,
            Some(&db_for_cache),
        )
        .await;
    });

    if let Some(pid) = paper_id {
        let docs_meta = docs.clone();
        let meta_db = db.clone();
        spawn(async move {
            crate::state::commands::extract_and_fetch_metadata(
                &docs_meta,
                &meta_db,
                &pid,
                &full_path,
                auto_fetch,
                &mut lib_state,
            )
            .await;
        });
    }
}

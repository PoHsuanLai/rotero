use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use rotero_db::Database;

#[cfg(feature = "desktop")]
fn download_pdf_menu_item(
    show: bool,
    paper_id: &str,
    pdf_url: Option<&str>,
    db: &Database,
    mut lib_state: Signal<LibraryState>,
) -> Element {
    if !show {
        return rsx! {};
    }
    let Some(url) = pdf_url else {
        return rsx! {};
    };
    let pdf_url = url.to_string();
    let pid = paper_id.to_string();
    let db = db.clone();
    // We need the paper clone for download — get it from state
    let paper_clone = lib_state.read().papers.iter().find(|p| p.id.as_deref() == Some(&pid)).cloned();
    let Some(paper_clone) = paper_clone else {
        return rsx! {};
    };
    rsx! {
        ContextMenuItem {
            label: "Download PDF".to_string(),
            icon: Some("bi-download".to_string()),
            on_click: move |_| {
                let pdf_url = pdf_url.clone();
                let paper_clone = paper_clone.clone();
                let pid = pid.clone();
                let db = db.clone();
                spawn(async move {
                    let lib_path = db.data_dir().join("pdfs");
                    match crate::download_and_import_pdf(
                        db.conn(),
                        &lib_path,
                        &pid,
                        &paper_clone,
                        &pdf_url,
                    ).await {
                        Ok(()) => {
                            crate::state::commands::refresh_papers(db.conn(), &mut lib_state).await;
                        }
                        Err(e) => {
                            tracing::error!("Download PDF failed: {e}");
                        }
                    }
                });
            },
        }
    }
}

#[cfg(not(feature = "desktop"))]
fn download_pdf_menu_item(
    _show: bool,
    _paper_id: &str,
    _pdf_url: Option<&str>,
    _db: &Database,
    _lib_state: Signal<LibraryState>,
) -> Element {
    rsx! {}
}

/// Context menu shown when right-clicking paper(s) in the library list.
/// When multiple papers are selected, bulk actions are shown.
#[component]
pub fn PaperContextMenu(
    paper_ids: Vec<String>,
    x: f64,
    y: f64,
    on_close: EventHandler<()>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();

    let is_multi = paper_ids.len() > 1;
    let count = paper_ids.len();

    // For single select, get paper details
    let single_paper = if !is_multi {
        let state = lib_state.read();
        state.papers.iter().find(|p| p.id.as_deref() == Some(paper_ids[0].as_str())).cloned()
    } else {
        None
    };

    let db_fav = db.clone();
    let db_read = db.clone();
    let pids = paper_ids.clone();
    let pids_fav = paper_ids.clone();
    let pids_read = paper_ids.clone();
    let pids_del = paper_ids.clone();
    let pids_doi = paper_ids.clone();

    // Pre-compute labels outside rsx
    let fav_label = if is_multi {
        format!("Favorite {count} papers")
    } else {
        let is_fav = single_paper.as_ref().is_some_and(|p| p.status.is_favorite);
        if is_fav { "Unfavorite".to_string() } else { "Favorite".to_string() }
    };
    let fav_icon = if !is_multi && single_paper.as_ref().is_some_and(|p| p.status.is_favorite) {
        "bi-star-fill".to_string()
    } else {
        "bi-star".to_string()
    };
    let read_label = if is_multi {
        format!("Mark {count} as read")
    } else {
        let is_read = single_paper.as_ref().is_some_and(|p| p.status.is_read);
        if is_read { "Mark as unread".to_string() } else { "Mark as read".to_string() }
    };
    let read_icon = if !is_multi && single_paper.as_ref().is_some_and(|p| p.status.is_read) {
        "bi-book".to_string()
    } else {
        "bi-book-fill".to_string()
    };
    let delete_label = if is_multi { format!("Delete {count} papers") } else { "Delete".to_string() };

    // Collect DOIs
    let dois: Vec<String> = {
        let state = lib_state.read();
        pids_doi.iter()
            .filter_map(|pid| state.papers.iter().find(|p| p.id.as_deref() == Some(pid.as_str())))
            .filter_map(|p| p.doi.clone())
            .collect()
    };
    let has_dois = !dois.is_empty();
    let doi_label = if is_multi { format!("Copy {} DOIs", dois.len()) } else { "Copy DOI".to_string() };

    // Collection removal
    let remove_label = if is_multi { format!("Remove {count} from Collection") } else { "Remove from Collection".to_string() };
    let in_collection = matches!(lib_state.read().view, LibraryView::Collection(_));
    let collection_id = if let LibraryView::Collection(ref cid) = lib_state.read().view {
        Some(cid.clone())
    } else {
        None
    };

    rsx! {
        ContextMenu {
            x,
            y,
            on_close: move |_| on_close.call(()),

            // Single-paper-only actions
            if !is_multi {
                if let Some(ref paper) = single_paper {
                    if paper.links.pdf_path.is_some() {
                        {
                            let pid = pids[0].clone();
                            let pdf_rel = paper.links.pdf_path.clone();
                            let title = paper.title.clone();
                            let db_ctx = db.clone();
                            rsx! {
                                ContextMenuItem {
                                    label: "Open PDF".to_string(),
                                    icon: Some("bi-eye".to_string()),
                                    on_click: move |_| {
                                        if let Some(ref rel_path) = pdf_rel {
                                            crate::state::commands::open_paper_pdf(&db_ctx, &mut tabs, &mut lib_state, &config, &dpr_sig, &pid, rel_path, &title);
                                        }
                                    },
                                }
                            }
                        }
                    }

                    {download_pdf_menu_item(
                        paper.links.pdf_path.is_none(),
                        &pids[0],
                        paper.links.pdf_url.as_deref(),
                        &db,
                        lib_state,
                    )}
                }
            }

            // Favorite — works for single and multi
            ContextMenuItem {
                label: fav_label,
                icon: Some(fav_icon),
                on_click: move |_| {
                    let db = db_fav.clone();
                    let pids = pids_fav.clone();
                    spawn(async move {
                        let state = lib_state.read();
                        // For single: toggle. For multi: always set favorite.
                        let new_val = if pids.len() == 1 {
                            !state.papers.iter().find(|p| p.id.as_deref() == Some(pids[0].as_str())).map(|p| p.status.is_favorite).unwrap_or(false)
                        } else {
                            true
                        };
                        drop(state);
                        for pid in &pids {
                            let _ = rotero_db::papers::set_favorite(db.conn(), pid, new_val).await;
                        }
                        lib_state.with_mut(|s| {
                            for pid in &pids {
                                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid.as_str())) {
                                    p.status.is_favorite = new_val;
                                }
                            }
                        });
                    });
                },
            }

            // Read/Unread — works for single and multi
            ContextMenuItem {
                label: read_label,
                icon: Some(read_icon),
                on_click: move |_| {
                    let db = db_read.clone();
                    let pids = pids_read.clone();
                    spawn(async move {
                        let state = lib_state.read();
                        let new_val = if pids.len() == 1 {
                            !state.papers.iter().find(|p| p.id.as_deref() == Some(pids[0].as_str())).map(|p| p.status.is_read).unwrap_or(false)
                        } else {
                            true
                        };
                        drop(state);
                        for pid in &pids {
                            let _ = rotero_db::papers::set_read(db.conn(), pid, new_val).await;
                        }
                        lib_state.with_mut(|s| {
                            for pid in &pids {
                                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid.as_str())) {
                                    p.status.is_read = new_val;
                                }
                            }
                        });
                    });
                },
            }

            // Add Tag — single only (needs detail panel focus)
            if !is_multi {
                ContextMenuItem {
                    label: "Add Tag".to_string(),
                    icon: Some("bi-tag".to_string()),
                    on_click: {
                        let pid = pids[0].clone();
                        move |_| {
                            lib_state.with_mut(|s| {
                                s.select_one(pid.clone());
                            });
                            document::eval("setTimeout(() => { let el = document.getElementById('tag-editor-input'); if (el) el.focus(); }, 100)");
                        }
                    },
                }
            }

            ContextMenuSeparator {}

            // Copy DOI(s)
            if has_dois {
                ContextMenuItem {
                    label: doi_label,
                    icon: Some("bi-link-45deg".to_string()),
                    on_click: move |_| {
                        if let Ok(mut clip) = arboard::Clipboard::new() {
                            let _ = clip.set_text(dois.join("\n"));
                        }
                    },
                }
            }

            // Remove from Collection
            if in_collection {
                {
                    let db_remove = db.clone();
                    let pids = pids.clone();
                    let cid = collection_id.clone().unwrap_or_default();
                    rsx! {
                        ContextMenuItem {
                            label: remove_label,
                            icon: Some("bi-folder-minus".to_string()),
                            on_click: move |_| {
                                let db = db_remove.clone();
                                let pids = pids.clone();
                                let cid = cid.clone();
                                spawn(async move {
                                    for pid in &pids {
                                        let _ = rotero_db::collections::remove_paper_from_collection(db.conn(), pid, &cid).await;
                                    }
                                    if let Ok(ids) = rotero_db::collections::list_paper_ids_in_collection(db.conn(), &cid).await {
                                        lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                                    }
                                });
                            },
                        }
                    }
                }
            }

            ContextMenuSeparator {}

            // Delete — triggers confirmation dialog
            ContextMenuItem {
                label: delete_label,
                icon: Some("bi-trash".to_string()),
                danger: Some(true),
                on_click: move |_| {
                    lib_state.with_mut(|s| {
                        s.confirm_delete = Some(pids_del.clone());
                    });
                },
            }
        }
    }
}

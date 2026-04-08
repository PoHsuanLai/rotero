use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use rotero_db::Database;
use rotero_models::Paper;

#[cfg(feature = "desktop")]
fn download_pdf_menu_item(
    show: bool,
    paper: &Paper,
    paper_id: &str,
    db: &Database,
    mut lib_state: Signal<LibraryState>,
) -> Element {
    if !show || paper.links.pdf_url.is_none() {
        return rsx! {};
    }
    let pdf_url = paper.links.pdf_url.clone().unwrap_or_default();
    let paper_clone = paper.clone();
    let pid = paper_id.to_string();
    let db = db.clone();
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
    _paper: &Paper,
    _paper_id: &str,
    _db: &Database,
    _lib_state: Signal<LibraryState>,
) -> Element {
    rsx! {}
}

/// Context menu shown when right-clicking a paper in the library list.
#[component]
pub fn PaperContextMenu(
    paper: Paper,
    paper_id: String,
    x: f64,
    y: f64,
    on_close: EventHandler<()>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();

    let has_pdf = paper.links.pdf_path.is_some();
    let is_fav = paper.status.is_favorite;
    let is_read = paper.status.is_read;
    let doi = paper.doi.clone();
    let pdf_rel = paper.links.pdf_path.clone();
    let paper_title_for_open = paper.title.clone();
    let pid = paper_id;
    let db_ctx = db.clone();
    let db_fav = db.clone();
    let db_read = db.clone();
    let db_del = db.clone();

    rsx! {
        ContextMenu {
            x,
            y,
            on_close: move |_| on_close.call(()),

            if has_pdf {
                ContextMenuItem {
                    label: "Open PDF".to_string(),
                    icon: Some("bi-eye".to_string()),
                    on_click: {
                        let pid = pid.clone();
                        move |_| {
                            if let Some(ref rel_path) = pdf_rel {
                                crate::state::commands::open_paper_pdf(&db_ctx, &mut tabs, &mut lib_state, &config, &dpr_sig, &pid, rel_path, &paper_title_for_open);
                            }
                        }
                    },
                }
            }

            {download_pdf_menu_item(!has_pdf, &paper, &pid, &db, lib_state)}

            ContextMenuItem {
                label: if is_fav { "Unfavorite".to_string() } else { "Favorite".to_string() },
                icon: Some(if is_fav { "bi-star-fill".to_string() } else { "bi-star".to_string() }),
                on_click: {
                    let pid = pid.clone();
                    move |_| {
                        let db = db_fav.clone();
                        let new_val = !is_fav;
                        let pid = pid.clone();
                        spawn(async move {
                            if let Ok(()) = rotero_db::papers::set_favorite(db.conn(), &pid, new_val).await {
                                let pid2 = pid.clone();
                                lib_state.with_mut(|s| {
                                    if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                        p.status.is_favorite = new_val;
                                    }
                                });
                            }
                        });
                    }
                },
            }

            ContextMenuItem {
                label: if is_read { "Mark as unread".to_string() } else { "Mark as read".to_string() },
                icon: Some(if is_read { "bi-book".to_string() } else { "bi-book-fill".to_string() }),
                on_click: {
                    let pid = pid.clone();
                    move |_| {
                        let db = db_read.clone();
                        let new_val = !is_read;
                        let pid = pid.clone();
                        spawn(async move {
                            if let Ok(()) = rotero_db::papers::set_read(db.conn(), &pid, new_val).await {
                                let pid2 = pid.clone();
                                lib_state.with_mut(|s| {
                                    if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                        p.status.is_read = new_val;
                                    }
                                });
                            }
                        });
                    }
                },
            }

            ContextMenuItem {
                label: "Add Tag".to_string(),
                icon: Some("bi-tag".to_string()),
                on_click: {
                    let pid = pid.clone();
                    move |_| {
                        lib_state.with_mut(|s| {
                            s.selected_paper_id = Some(pid.clone());
                        });
                        document::eval("setTimeout(() => { let el = document.getElementById('tag-editor-input'); if (el) el.focus(); }, 100)");
                    }
                },
            }

            ContextMenuSeparator {}

            if let Some(ref doi_val) = doi {
                {
                    let doi_copy = doi_val.clone();
                    rsx! {
                        ContextMenuItem {
                            label: "Copy DOI".to_string(),
                            icon: Some("bi-link-45deg".to_string()),
                            on_click: move |_| {
                                if let Ok(mut clip) = arboard::Clipboard::new() {
                                    let _ = clip.set_text(&*doi_copy);
                                }
                            },
                        }
                    }
                }
            }

            if let LibraryView::Collection(ref coll_id) = lib_state.read().view.clone() {
                {
                    let db_remove = db.clone();
                    let pid = pid.clone();
                    let cid = coll_id.clone();
                    rsx! {
                        ContextMenuItem {
                            label: "Remove from Collection".to_string(),
                            icon: Some("bi-folder-minus".to_string()),
                            on_click: move |_| {
                                let db = db_remove.clone();
                                let pid = pid.clone();
                                let cid = cid.clone();
                                spawn(async move {
                                    if let Ok(()) = rotero_db::collections::remove_paper_from_collection(db.conn(), &pid, &cid).await
                                        && let Ok(ids) = rotero_db::collections::list_paper_ids_in_collection(db.conn(), &cid).await {
                                            lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                                        }
                                });
                            },
                        }
                    }
                }
            }

            ContextMenuSeparator {}

            ContextMenuItem {
                label: "Delete".to_string(),
                icon: Some("bi-trash".to_string()),
                danger: Some(true),
                on_click: {
                    let pid = pid.clone();
                    move |_| {
                        let db = db_del.clone();
                        let pid = pid.clone();
                        spawn(async move {
                            if let Ok(()) = rotero_db::papers::delete_paper(db.conn(), &pid).await {
                                let pid2 = pid.clone();
                                lib_state.with_mut(|s| {
                                    s.papers.retain(|p| p.id.as_deref() != Some(pid2.as_str()));
                                    if s.selected_paper_id.as_deref() == Some(pid.as_str()) {
                                        s.selected_paper_id = None;
                                    }
                                });
                            }
                        });
                    }
                },
            }
        }
    }
}

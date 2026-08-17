use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, LibraryView, MembershipRefresh, PdfTabManager};
use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use rotero_db::Database;

/// Which nested picker the paper context menu is currently showing, if any.
#[derive(Clone, Copy, PartialEq)]
enum Submenu {
    Collections,
    Tags,
}

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
    let paper_clone = lib_state
        .read()
        .papers
        .iter()
        .find(|p| p.id.as_deref() == Some(&pid))
        .cloned();
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
                        &db,
                        &lib_path,
                        &pid,
                        &paper_clone,
                        &pdf_url,
                    ).await {
                        Ok(()) => {
                            crate::state::commands::refresh_papers(&db, &mut lib_state).await;
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
    let refresh = use_context::<Signal<MembershipRefresh>>();

    // The "Add to Collection" / "Add Tag" rows swap the menu body to a picker.
    // The component can't nest true flyouts, so it mirrors the sidebar's
    // in-place expand pattern with a back button.
    let submenu = use_signal(|| None::<Submenu>);

    let is_multi = paper_ids.len() > 1;
    let count = paper_ids.len();

    // For single select, get paper details
    let single_paper = if !is_multi {
        let state = lib_state.read();
        state
            .papers
            .iter()
            .find(|p| p.id.as_deref() == Some(paper_ids[0].as_str()))
            .cloned()
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
        if is_fav {
            "Unfavorite".to_string()
        } else {
            "Favorite".to_string()
        }
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
        if is_read {
            "Mark as unread".to_string()
        } else {
            "Mark as read".to_string()
        }
    };
    let read_icon = if !is_multi && single_paper.as_ref().is_some_and(|p| p.status.is_read) {
        "bi-book".to_string()
    } else {
        "bi-book-fill".to_string()
    };
    let delete_label = if is_multi {
        format!("Delete {count} papers")
    } else {
        "Delete".to_string()
    };

    // Collect DOIs
    let dois: Vec<String> = {
        let state = lib_state.read();
        pids_doi
            .iter()
            .filter_map(|pid| {
                state
                    .papers
                    .iter()
                    .find(|p| p.id.as_deref() == Some(pid.as_str()))
            })
            .filter_map(|p| p.doi.clone())
            .collect()
    };
    let has_dois = !dois.is_empty();
    let doi_label = if is_multi {
        format!("Copy {} DOIs", dois.len())
    } else {
        "Copy DOI".to_string()
    };

    // Collection removal
    let remove_label = if is_multi {
        format!("Remove {count} from Collection")
    } else {
        "Remove from Collection".to_string()
    };
    let in_collection = matches!(lib_state.read().view, LibraryView::Collection(_));
    let collection_id = if let LibraryView::Collection(ref cid) = lib_state.read().view {
        Some(cid.clone())
    } else {
        None
    };

    // When a picker is open, swap the whole menu body for it.
    if let Some(sub) = submenu() {
        return rsx! {
            SubmenuPicker {
                which: sub,
                paper_ids: paper_ids.clone(),
                x,
                y,
                submenu,
                refresh,
                on_close: move |_| on_close.call(()),
            }
        };
    }

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
                            if let Err(e) = db.set_favorite(pid, new_val).await {
                                let mut lib_state = lib_state;
                                lib_state.with_mut(|s| s.report_error(format!("Could not update the favourite flag: {e}")));
                            }
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
                            if let Err(e) = db.set_read(pid, new_val).await {
                                let mut lib_state = lib_state;
                                lib_state.with_mut(|s| s.report_error(format!("Could not update the read flag: {e}")));
                            }
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

            // Add to Collection / Add Tag — open an in-place picker submenu.
            // Works for single and multi selection.
            ContextMenuItem {
                label: "Add to Collection".to_string(),
                icon: Some("bi-folder-plus".to_string()),
                // Keep the menu mounted; opening the submenu closes it otherwise.
                close_on_click: Some(false),
                on_click: {
                    let mut submenu = submenu;
                    move |_| submenu.set(Some(Submenu::Collections))
                },
            }
            ContextMenuItem {
                label: "Add Tag".to_string(),
                icon: Some("bi-tag".to_string()),
                close_on_click: Some(false),
                on_click: {
                    let mut submenu = submenu;
                    move |_| submenu.set(Some(Submenu::Tags))
                },
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
                            on_click: {
                                let mut refresh = refresh;
                                move |_| {
                                let db = db_remove.clone();
                                let pids = pids.clone();
                                let cid = cid.clone();
                                spawn(async move {
                                    for pid in &pids {
                                        if let Err(e) = db.remove_paper_from_collection(pid, &cid).await {
                                            let mut lib_state = lib_state;
                                            lib_state.with_mut(|s| s.report_error(format!("Could not remove the paper from that collection: {e}")));
                                        }
                                    }
                                    if let Ok(ids) = db.list_paper_ids_in_subtree(&cid).await {
                                        lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                                    }
                                    refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                });
                            }},
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

/// The in-place "Add to Collection" / "Add Tag" picker. Reuses the same
/// `ContextMenu` chrome as the parent, opening with a "‹ Back" row. Adds apply
/// to every paper in the selection and are idempotent, so a paper already in the
/// collection/tag is a no-op.
#[component]
fn SubmenuPicker(
    which: Submenu,
    paper_ids: Vec<String>,
    x: f64,
    y: f64,
    submenu: Signal<Option<Submenu>>,
    refresh: Signal<MembershipRefresh>,
    on_close: EventHandler<()>,
) -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut new_tag = use_signal(String::new);

    let collections = lib_state.read().collections.clone();
    let tags = lib_state.read().tags.clone();

    rsx! {
        ContextMenu {
            x,
            y,
            on_close: move |_| on_close.call(()),

            // Back to the main menu (does not close the menu).
            ContextMenuItem {
                label: match which {
                    Submenu::Collections => "Add to Collection".to_string(),
                    Submenu::Tags => "Add Tag".to_string(),
                },
                icon: Some("bi-chevron-left".to_string()),
                close_on_click: Some(false),
                on_click: {
                    let mut submenu = submenu;
                    move |_| submenu.set(None)
                },
            }

            ContextMenuSeparator {}

            div { class: "context-menu-scroll",
                match which {
                    Submenu::Collections => rsx! {
                        if collections.is_empty() {
                            div { class: "context-menu-empty", "No collections yet" }
                        }
                        for coll in collections.iter() {
                            {
                                let cid = coll.id.clone().unwrap_or_default();
                                let cname = coll.name.clone();
                                let db = db.clone();
                                let pids = paper_ids.clone();
                                let mut refresh = refresh;
                                rsx! {
                                    ContextMenuItem {
                                        label: cname,
                                        icon: Some("bi-folder".to_string()),
                                        close_on_click: Some(false),
                                        on_click: move |_| {
                                            let db = db.clone();
                                            let pids = pids.clone();
                                            let cid = cid.clone();
                                            spawn(async move {
                                                for pid in &pids {
                                                    if let Err(e) = db.add_paper_to_collection(pid, &cid).await {
                                                        let mut lib_state = lib_state;
                                                        lib_state.with_mut(|s| s.report_error(format!("Could not add the paper to that collection: {e}")));
                                                    }
                                                }
                                                refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                                on_close.call(());
                                            });
                                        },
                                    }
                                }
                            }
                        }
                    },
                    Submenu::Tags => rsx! {
                        div { class: "context-menu-rename",
                            input {
                                class: "input input--sm",
                                r#type: "text",
                                placeholder: "New tag...",
                                autofocus: true,
                                value: "{new_tag}",
                                onfocusin: crate::ui::keybindings::editable_focus_in,
                                onfocusout: crate::ui::keybindings::editable_focus_out,
                                oninput: move |evt| new_tag.set(evt.value()),
                                onkeydown: {
                                    let db = db.clone();
                                    let pids = paper_ids.clone();
                                    let mut refresh = refresh;
                                    move |evt: Event<KeyboardData>| {
                                        if evt.key() == Key::Enter {
                                            let name = new_tag().trim().to_string();
                                            if name.is_empty() { return; }
                                            let db = db.clone();
                                            let pids = pids.clone();
                                            spawn(async move {
                                                if let Ok(tid) = db.get_or_create_tag(&name, None).await {
                                                    for pid in &pids {
                                                        if let Err(e) = db.add_tag_to_paper(pid, &tid).await {
                                                            let mut lib_state = lib_state;
                                                            lib_state.with_mut(|s| s.report_error(format!("Could not attach that tag: {e}")));
                                                        }
                                                    }
                                                    refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                                }
                                                on_close.call(());
                                            });
                                        }
                                    }
                                },
                            }
                        }
                        if !tags.is_empty() {
                            ContextMenuSeparator {}
                        }
                        for tag in tags.iter() {
                            {
                                let tid = tag.id.clone().unwrap_or_default();
                                let tname = tag.name.clone();
                                let bg = tag.color.clone().unwrap_or_else(|| "#6b7085".to_string());
                                let db = db.clone();
                                let pids = paper_ids.clone();
                                let mut refresh = refresh;
                                rsx! {
                                    div {
                                        class: "context-menu-item",
                                        onclick: move |evt: Event<MouseData>| {
                                            evt.stop_propagation();
                                            let db = db.clone();
                                            let pids = pids.clone();
                                            let tid = tid.clone();
                                            spawn(async move {
                                                for pid in &pids {
                                                    if let Err(e) = db.add_tag_to_paper(pid, &tid).await {
                                                        let mut lib_state = lib_state;
                                                        lib_state.with_mut(|s| s.report_error(format!("Could not attach that tag: {e}")));
                                                    }
                                                }
                                                refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                                on_close.call(());
                                            });
                                        },
                                        span { class: "context-menu-tag-dot", style: "background: {bg};" }
                                        span { class: "context-menu-label", "{tname}" }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

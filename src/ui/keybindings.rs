use dioxus::prelude::*;
use dioxus::desktop::use_global_shortcut;

use crate::app::{RenderChannel, ShowSettings};
use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfTab, PdfTabManager};
use crate::state::undo::{UndoStack, reverse_action, forward_action};
use crate::sync::engine::SyncConfig;

/// Registers global keyboard shortcuts using Dioxus desktop's native shortcut API.
#[component]
pub fn GlobalKeyHandler() -> Element {
    let mut show_settings = use_context::<Signal<ShowSettings>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<SyncConfig>>();
    let mut new_coll_editing = use_context::<Signal<Option<Option<i64>>>>();
    let mut undo_stack = use_context::<Signal<UndoStack>>();

    // Cmd+, → Open Settings (Escape to close)
    let _ = use_global_shortcut("CmdOrCtrl+,", move || {
        tracing::info!("Shortcut: Cmd+, (open settings)");
        show_settings.set(ShowSettings(true));
    });

    // Cmd+O → Open PDF
    let _ = use_global_shortcut("CmdOrCtrl+O", move || {
        tracing::info!("Shortcut: Cmd+O (open PDF)");
        let file = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_title("Open PDF")
            .pick_file();

        if let Some(path) = file {
            let path_str = path.to_string_lossy().to_string();
            tabs.with_mut(|m| {
                if let Some(idx) = m.find_by_path(&path_str) {
                    let tid = m.tabs[idx].id;
                    m.switch_to(tid);
                } else {
                    let cfg = config.read();
                    let id = m.next_id();
                    let title = std::path::Path::new(&path_str)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Untitled".to_string());
                    let tab = PdfTab::new(id, path_str.clone(), title, cfg.default_zoom, cfg.page_batch_size);
                    m.open_tab(tab);
                }
            });
            lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
        }
    });

    // Cmd+I → Import BibTeX
    let db_import = db.clone();
    let _ = use_global_shortcut("CmdOrCtrl+I", move || {
        tracing::info!("Shortcut: Cmd+I (import BibTeX)");
        let file = rfd::FileDialog::new()
            .add_filter("BibTeX", &["bib", "bibtex"])
            .set_title("Import BibTeX")
            .pick_file();

        if let Some(path) = file {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(papers) = rotero_bib::import_bibtex(&content) {
                    let db = db_import.clone();
                    spawn(async move {
                        for paper in papers {
                            if let Ok(id) = crate::db::papers::insert_paper(db.conn(), &paper).await {
                                let mut paper = paper;
                                paper.id = Some(id);
                                lib_state.with_mut(|s| s.papers.insert(0, paper));
                            }
                        }
                    });
                }
            }
        }
    });

    // Cmd+E → Export BibTeX
    let _ = use_global_shortcut("CmdOrCtrl+E", move || {
        tracing::info!("Shortcut: Cmd+E (export BibTeX)");
        let papers = lib_state.read().papers.clone();
        if !papers.is_empty() {
            let file = rfd::FileDialog::new()
                .add_filter("BibTeX", &["bib"])
                .set_title("Export BibTeX")
                .set_file_name("rotero-export.bib")
                .save_file();

            if let Some(path) = file {
                let bibtex = rotero_bib::export_bibtex(&papers);
                let _ = std::fs::write(&path, bibtex);
            }
        }
    });

    // Cmd+F → Search (context-dependent: library search bar or PDF in-document search)
    let _ = use_global_shortcut("CmdOrCtrl+F", move || {
        tracing::info!("Shortcut: Cmd+F (search)");
        let view = lib_state.read().view.clone();
        if view == LibraryView::PdfViewer {
            // Toggle PDF search bar
            tabs.with_mut(|m| {
                if let Some(t) = m.active_tab_mut() {
                    t.search.visible = !t.search.visible;
                    if !t.search.visible {
                        t.search.query.clear();
                        t.search.matches.clear();
                        t.search.current_index = 0;
                    }
                }
            });
        } else {
            // Focus library search bar
            spawn(async move {
                let _ = document::eval(
                    "document.getElementById('library-search-input')?.focus()"
                );
            });
        }
    });

    // Cmd+L → Focus library search
    let _ = use_global_shortcut("CmdOrCtrl+L", move || {
        tracing::info!("Shortcut: Cmd+L (focus search)");
        let view = lib_state.read().view.clone();
        if view == LibraryView::PdfViewer {
            lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
        }
        spawn(async move {
            let _ = document::eval(
                "setTimeout(() => { document.getElementById('library-search-input')?.focus(); }, 50)"
            );
        });
    });

    // Cmd+W → Close active PDF tab
    let _ = use_global_shortcut("CmdOrCtrl+W", move || {
        tracing::info!("Shortcut: Cmd+W (close tab)");
        let has_active = tabs.read().active_tab_id.is_some();
        if has_active {
            let tab_id = tabs.read().active_tab_id.unwrap();
            tabs.with_mut(|m| { m.close_tab(tab_id); });
            if tabs.read().tabs.is_empty() {
                lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
            } else {
                let needs = tabs.read().active_tab().map(|t| t.needs_render()).unwrap_or(false);
                if needs {
                    let new_id = tabs.read().active_tab_id.unwrap();
                    let render_tx = render_ch.sender();
                    let cfg_dir = config.read().effective_library_path();
                    let cfg_q = config.read().render_quality;
                    tabs.with_mut(|m| m.tab_mut().is_loading = true);
                    spawn(async move {
                        let _ = crate::state::commands::open_pdf(&render_tx, &mut tabs, new_id, &cfg_dir, cfg_q).await;
                    });
                }
            }
        }
    });

    // Cmd+N → New collection
    let _ = use_global_shortcut("CmdOrCtrl+N", move || {
        tracing::info!("Shortcut: Cmd+N (new collection)");
        let parent = match lib_state.read().view {
            LibraryView::Collection(id) => Some(id),
            _ => None,
        };
        new_coll_editing.set(Some(parent));
    });

    // Cmd+1 → Go to Library view
    let _ = use_global_shortcut("CmdOrCtrl+1", move || {
        tracing::info!("Shortcut: Cmd+1 (library view)");
        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
    });

    // Cmd+[ → Previous PDF tab
    let _ = use_global_shortcut("CmdOrCtrl+[", move || {
        tracing::info!("Shortcut: Cmd+[ (prev tab)");
        tabs.with_mut(|m| {
            if let Some(active_id) = m.active_tab_id {
                if let Some(idx) = m.tabs.iter().position(|t| t.id == active_id) {
                    if idx > 0 {
                        let prev_id = m.tabs[idx - 1].id;
                        m.switch_to(prev_id);
                    }
                }
            }
        });
        if lib_state.read().view != LibraryView::PdfViewer {
            lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
        }
    });

    // Cmd+] → Next PDF tab
    let _ = use_global_shortcut("CmdOrCtrl+]", move || {
        tracing::info!("Shortcut: Cmd+] (next tab)");
        tabs.with_mut(|m| {
            if let Some(active_id) = m.active_tab_id {
                if let Some(idx) = m.tabs.iter().position(|t| t.id == active_id) {
                    if idx + 1 < m.tabs.len() {
                        let next_id = m.tabs[idx + 1].id;
                        m.switch_to(next_id);
                    }
                }
            }
        });
        if lib_state.read().view != LibraryView::PdfViewer {
            lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
        }
    });

    // Cmd+Z → Undo annotation action (reverse the action)
    let db_undo = db.clone();
    let _ = use_global_shortcut("CmdOrCtrl+Z", move || {
        let action = undo_stack.with_mut(|s| s.pop_undo());
        if let Some(action) = action {
            let db = db_undo.clone();
            spawn(async move {
                reverse_action(db, &mut tabs, action).await;
            });
        }
    });

    // Cmd+Shift+Z → Redo annotation action (re-apply the action)
    let db_redo = db.clone();
    let _ = use_global_shortcut("CmdOrCtrl+Shift+Z", move || {
        let action = undo_stack.with_mut(|s| s.pop_redo());
        if let Some(action) = action {
            let db = db_redo.clone();
            spawn(async move {
                forward_action(db, &mut tabs, action).await;
            });
        }
    });

    // Escape → Close settings
    let _ = use_global_shortcut("Escape", move || {
        tracing::info!("Shortcut: Escape");
        if show_settings.read().0 {
            show_settings.set(ShowSettings(false));
        }
    });

    rsx! {}
}

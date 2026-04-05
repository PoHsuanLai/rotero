use dioxus::desktop::{use_global_shortcut, use_muda_event_handler};
use dioxus::prelude::*;

use crate::app::{DevicePixelRatio, RenderChannel, ShowSettings};
use crate::state::app_state::{
    AnnotationMode, LibraryState, LibraryView, PdfTab, PdfTabManager, ViewerToolState,
};
use crate::state::undo::{UndoStack, forward_action, reverse_action};
use crate::sync::engine::SyncConfig;
use rotero_db::Database;

// ── Action functions ──────────────────────────────────────────────────
// Each action is a plain function so both keyboard shortcuts and menu
// events can call the same logic without duplication.

fn action_open_settings(mut show_settings: Signal<ShowSettings>) {
    show_settings.set(ShowSettings(true));
}

fn action_open_pdf(
    mut tabs: Signal<PdfTabManager>,
    mut lib_state: Signal<LibraryState>,
    config: Signal<SyncConfig>,
    dpr_sig: Signal<DevicePixelRatio>,
) {
    let file = crate::ui::pick_file(&["pdf"], "Open PDF");
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
                let tab = PdfTab::new(
                    id,
                    path_str.clone(),
                    title,
                    cfg.default_zoom,
                    cfg.page_batch_size,
                    dpr_sig.read().0,
                );
                m.open_tab(tab);
            }
        });
        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
    }
}

fn action_import_bibtex(db: Database, mut lib_state: Signal<LibraryState>) {
    let file = crate::ui::pick_file(&["bib", "bibtex"], "Import BibTeX");
    if let Some(path) = file
        && let Ok(content) = std::fs::read_to_string(&path)
        && let Ok(papers) = rotero_bib::import_bibtex(&content)
    {
        spawn(async move {
            for paper in papers {
                if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                    let mut paper = paper;
                    paper.id = Some(id);
                    lib_state.with_mut(|s| s.papers.insert(0, paper));
                }
            }
        });
    }
}

fn action_export_bibtex(lib_state: Signal<LibraryState>) {
    let papers = lib_state.read().papers.clone();
    if !papers.is_empty() {
        let file = crate::ui::save_file(&["bib"], "Export BibTeX", "rotero-export.bib");
        if let Some(path) = file {
            let bibtex = rotero_bib::export_bibtex(&papers);
            let _ = std::fs::write(&path, bibtex);
        }
    }
}

fn action_find(lib_state: Signal<LibraryState>, mut tabs: Signal<PdfTabManager>) {
    let view = lib_state.read().view.clone();
    if view == LibraryView::PdfViewer {
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
        spawn(async move {
            let _ = document::eval("document.getElementById('library-search-input')?.focus()");
        });
    }
}

fn action_focus_library_search(mut lib_state: Signal<LibraryState>) {
    let view = lib_state.read().view.clone();
    if view == LibraryView::PdfViewer {
        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
    }
    spawn(async move {
        let _ = document::eval(
            "setTimeout(() => { document.getElementById('library-search-input')?.focus(); }, 50)",
        );
    });
}

fn action_close_tab(
    mut tabs: Signal<PdfTabManager>,
    mut lib_state: Signal<LibraryState>,
    render_ch: RenderChannel,
    config: Signal<SyncConfig>,
) {
    let Some(tab_id) = tabs.read().active_tab_id else {
        return;
    };
    tabs.with_mut(|m| m.close_tab(tab_id));
    if tabs.read().tabs.is_empty() {
        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
    } else {
        let needs = tabs
            .read()
            .active_tab()
            .map(|t| t.needs_render())
            .unwrap_or(false);
        let new_id = tabs.read().active_tab_id;
        if needs && let Some(new_id) = new_id {
            let render_tx = render_ch.sender();
            let cfg_dir = config.read().effective_library_path();
            let cfg_q = config.read().render_quality;
            let cfg_fmt = rotero_pdf::RenderFormat::from_str(&config.read().render_format);
            tabs.with_mut(|m| m.tab_mut().is_loading = true);
            spawn(async move {
                let _ = crate::state::commands::open_pdf(
                    &render_tx, &mut tabs, new_id, &cfg_dir, cfg_q, cfg_fmt,
                )
                .await;
            });
        }
    }
}

fn action_new_collection(
    lib_state: Signal<LibraryState>,
    mut new_coll_editing: Signal<Option<Option<String>>>,
) {
    let parent = match &lib_state.read().view {
        LibraryView::Collection(id) => Some(id.clone()),
        _ => None,
    };
    new_coll_editing.set(Some(parent));
}

fn action_show_library(mut lib_state: Signal<LibraryState>) {
    lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
}

fn action_prev_tab(mut tabs: Signal<PdfTabManager>, mut lib_state: Signal<LibraryState>) {
    tabs.with_mut(|m| {
        if let Some(active_id) = m.active_tab_id
            && let Some(idx) = m.tabs.iter().position(|t| t.id == active_id)
            && idx > 0
        {
            let prev_id = m.tabs[idx - 1].id;
            m.switch_to(prev_id);
        }
    });
    if lib_state.read().view != LibraryView::PdfViewer {
        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
    }
}

fn action_next_tab(mut tabs: Signal<PdfTabManager>, mut lib_state: Signal<LibraryState>) {
    tabs.with_mut(|m| {
        if let Some(active_id) = m.active_tab_id
            && let Some(idx) = m.tabs.iter().position(|t| t.id == active_id)
            && idx + 1 < m.tabs.len()
        {
            let next_id = m.tabs[idx + 1].id;
            m.switch_to(next_id);
        }
    });
    if lib_state.read().view != LibraryView::PdfViewer {
        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
    }
}

fn action_undo(db: Database, mut tabs: Signal<PdfTabManager>, mut undo_stack: Signal<UndoStack>) {
    let action = undo_stack.with_mut(|s| s.pop_undo());
    if let Some(action) = action {
        spawn(async move {
            reverse_action(db, &mut tabs, &mut undo_stack, action).await;
        });
    }
}

fn action_redo(db: Database, mut tabs: Signal<PdfTabManager>, mut undo_stack: Signal<UndoStack>) {
    let action = undo_stack.with_mut(|s| s.pop_redo());
    if let Some(action) = action {
        spawn(async move {
            forward_action(db, &mut tabs, &mut undo_stack, action).await;
        });
    }
}

fn action_escape(
    mut show_settings: Signal<ShowSettings>,
    mut tabs: Signal<PdfTabManager>,
    mut tools: Signal<ViewerToolState>,
) {
    let mode = tools.read().annotation_mode;
    if mode != AnnotationMode::None {
        tools.with_mut(|t| t.annotation_mode = AnnotationMode::None);
    } else if show_settings.read().0 {
        show_settings.set(ShowSettings(false));
    } else {
        tabs.with_mut(|m| {
            if let Some(t) = m.active_tab_mut()
                && t.search.visible
            {
                t.search.visible = false;
                t.search.query.clear();
                t.search.matches.clear();
                t.search.current_index = 0;
            }
        });
    }
}

// ── Component ─────────────────────────────────────────────────────────

/// Registers global keyboard shortcuts and native menu event handlers.
#[component]
pub fn GlobalKeyHandler() -> Element {
    let show_settings = use_context::<Signal<ShowSettings>>();
    let lib_state = use_context::<Signal<LibraryState>>();
    let tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<SyncConfig>>();
    let new_coll_editing = use_context::<Signal<Option<Option<String>>>>();
    let undo_stack = use_context::<Signal<UndoStack>>();
    let tools = use_context::<Signal<ViewerToolState>>();
    let dpr_sig = use_context::<Signal<DevicePixelRatio>>();

    // ── Keyboard shortcuts ────────────────────────────────────────────

    let _ = use_global_shortcut("CmdOrCtrl+,", move |_| {
        action_open_settings(show_settings);
    });

    let _ = use_global_shortcut("CmdOrCtrl+O", move |_| {
        action_open_pdf(tabs, lib_state, config, dpr_sig);
    });

    let db_import = db.clone();
    let _ = use_global_shortcut("CmdOrCtrl+I", move |_| {
        action_import_bibtex(db_import.clone(), lib_state);
    });

    let _ = use_global_shortcut("CmdOrCtrl+E", move |_| {
        action_export_bibtex(lib_state);
    });

    let _ = use_global_shortcut("CmdOrCtrl+F", move |_| {
        action_find(lib_state, tabs);
    });

    let _ = use_global_shortcut("CmdOrCtrl+L", move |_| {
        action_focus_library_search(lib_state);
    });

    let _ = use_global_shortcut("CmdOrCtrl+W", move |_| {
        action_close_tab(tabs, lib_state, render_ch, config);
    });

    let _ = use_global_shortcut("CmdOrCtrl+N", move |_| {
        action_new_collection(lib_state, new_coll_editing);
    });

    let _ = use_global_shortcut("CmdOrCtrl+1", move |_| {
        action_show_library(lib_state);
    });

    let _ = use_global_shortcut("CmdOrCtrl+[", move |_| {
        action_prev_tab(tabs, lib_state);
    });

    let _ = use_global_shortcut("CmdOrCtrl+]", move |_| {
        action_next_tab(tabs, lib_state);
    });

    let db_undo = db.clone();
    let _ = use_global_shortcut("CmdOrCtrl+Z", move |_| {
        action_undo(db_undo.clone(), tabs, undo_stack);
    });

    let db_redo = db.clone();
    let _ = use_global_shortcut("CmdOrCtrl+Shift+Z", move |_| {
        action_redo(db_redo.clone(), tabs, undo_stack);
    });

    let _ = use_global_shortcut("Escape", move |_| {
        action_escape(show_settings, tabs, tools);
    });

    // ── Native menu event handler ─────────────────────────────────────

    let db_menu = db.clone();
    let _ = use_muda_event_handler(move |event| match event.id().0.as_str() {
        "open-pdf" => action_open_pdf(tabs, lib_state, config, dpr_sig),
        "import-bibtex" => action_import_bibtex(db_menu.clone(), lib_state),
        "export-bibtex" => action_export_bibtex(lib_state),
        "close-tab" => action_close_tab(tabs, lib_state, render_ch, config),
        "find" => action_find(lib_state, tabs),
        "show-library" => action_show_library(lib_state),
        "new-collection" => action_new_collection(lib_state, new_coll_editing),
        _ => {}
    });

    rsx! {}
}

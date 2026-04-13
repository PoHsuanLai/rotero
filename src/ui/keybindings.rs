use dioxus::desktop::use_muda_event_handler;
use dioxus::prelude::*;

use crate::app::{DevicePixelRatio, RenderChannel, ShowSettings};
use crate::state::app_state::{
    AnnotationMode, LibraryState, LibraryView, PdfTab, PdfTabManager, ViewerToolState,
};
use crate::state::undo::{UndoStack, forward_action, reverse_action};
use crate::sync::engine::SyncConfig;
use crate::updates::{UpdateState, UpdateStatus};
use rotero_db::Database;

fn action_open_settings(mut show_settings: Signal<ShowSettings>) {
    show_settings.set(ShowSettings(true));
}

fn action_check_updates(mut update_state: Signal<UpdateState>) {
    update_state.with_mut(|s| {
        s.status = UpdateStatus::Checking;
        s.show_dialog = true;
        s.error = None;
    });
    spawn(async move {
        match crate::updates::check_for_update().await {
            Ok(Some(info)) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::Available;
                    s.info = Some(info);
                });
            }
            Ok(None) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::UpToDate;
                });
            }
            Err(e) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::Error;
                    s.error = Some(e);
                });
            }
        }
    });
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
                    cfg.pdf.default_zoom,
                    cfg.pdf.page_batch_size,
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
        && let Ok(entries) = rotero_bib::import_bibtex(&content)
    {
        let bib_dir = path.parent().map(|p| p.to_path_buf());
        spawn(async move {
            for entry in entries {
                let rotero_bib::ImportedPaper { paper, source_pdf } = entry;
                if let Ok(id) = rotero_db::papers::insert_paper(db.conn(), &paper).await {
                    let mut paper = paper;
                    paper.id = Some(id.clone());

                    if let (Some(bib_dir), Some(rel_pdf)) = (&bib_dir, &source_pdf) {
                        let pdf_abs = bib_dir.join(rel_pdf);
                        if pdf_abs.exists()
                            && let Ok(rel_path) = db.import_pdf(
                                pdf_abs.to_str().unwrap_or_default(),
                                Some(paper.title.as_str()),
                                paper.authors.first().map(|a| a.as_str()),
                                paper.year,
                            )
                        {
                            let _ =
                                rotero_db::papers::update_pdf_path(db.conn(), &id, &rel_path).await;
                            paper.links.pdf_path = Some(rel_path);
                        }
                    }

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
    dpr_sig: Signal<DevicePixelRatio>,
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
            tabs.with_mut(|m| m.tab_mut().is_loading = true);
            spawn(async move {
                let _ = crate::state::commands::open_pdf(
                    &render_tx,
                    &mut tabs,
                    new_id,
                    &cfg_dir,
                    dpr_sig.read().0,
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
    mut lib_state: Signal<LibraryState>,
) {
    let mode = tools.read().annotation_mode;
    if mode != AnnotationMode::None {
        tools.with_mut(|t| t.annotation_mode = AnnotationMode::None);
    } else if show_settings.read().0 {
        show_settings.set(ShowSettings(false));
    } else {
        let mut handled = false;
        tabs.with_mut(|m| {
            if let Some(t) = m.active_tab_mut()
                && t.search.visible
            {
                t.search.visible = false;
                t.search.query.clear();
                t.search.matches.clear();
                t.search.current_index = 0;
                handled = true;
            }
        });
        if !handled && !lib_state.read().selected_paper_ids.is_empty() {
            lib_state.with_mut(|s| s.clear_selection());
        }
    }
}

fn action_select_next(mut lib_state: Signal<LibraryState>) {
    let ids = lib_state.read().filtered_paper_ids();
    if ids.is_empty() {
        return;
    }
    let current = lib_state.read().single_selected_id().cloned();
    let next = match current {
        Some(ref cid) => {
            let pos = ids.iter().position(|id| id == cid).unwrap_or(0);
            if pos + 1 < ids.len() { pos + 1 } else { pos }
        }
        None => 0,
    };
    lib_state.with_mut(|s| s.select_one(ids[next].clone()));
}

fn action_select_prev(mut lib_state: Signal<LibraryState>) {
    let ids = lib_state.read().filtered_paper_ids();
    if ids.is_empty() {
        return;
    }
    let current = lib_state.read().single_selected_id().cloned();
    let prev = match current {
        Some(ref cid) => {
            let pos = ids.iter().position(|id| id == cid).unwrap_or(0);
            if pos > 0 { pos - 1 } else { 0 }
        }
        None => 0,
    };
    lib_state.with_mut(|s| s.select_one(ids[prev].clone()));
}

fn action_select_all(mut lib_state: Signal<LibraryState>) {
    let ids = lib_state.read().filtered_paper_ids();
    lib_state.with_mut(|s| s.select_all(ids.into_iter()));
}

fn action_open_selected_pdf(
    mut lib_state: Signal<LibraryState>,
    mut tabs: Signal<PdfTabManager>,
    db: &Database,
    config: &Signal<SyncConfig>,
    dpr_sig: &Signal<DevicePixelRatio>,
) {
    let state = lib_state.read();
    if state.selection_count() != 1 {
        return;
    }
    let paper = state.selected_paper().cloned();
    drop(state);
    if let Some(paper) = paper
        && let Some(ref rel_path) = paper.links.pdf_path
    {
        let pid = paper.id.clone().unwrap_or_default();
        crate::state::commands::open_paper_pdf(db, &mut tabs, &mut lib_state, config, dpr_sig, &pid, rel_path, &paper.title);
    }
}

fn action_delete_selected(mut lib_state: Signal<LibraryState>) {
    let ids: Vec<String> = lib_state.read().selected_paper_ids.iter().cloned().collect();
    if !ids.is_empty() {
        lib_state.with_mut(|s| s.confirm_delete = Some(ids));
    }
}

fn action_toggle_favorite_selected(mut lib_state: Signal<LibraryState>, db: Database) {
    let ids: Vec<String> = lib_state.read().selected_paper_ids.iter().cloned().collect();
    if ids.is_empty() {
        return;
    }
    // For single: toggle. For multi: set all to favorite.
    let new_val = if ids.len() == 1 {
        !lib_state.read().papers.iter().find(|p| p.id.as_deref() == Some(ids[0].as_str())).map(|p| p.status.is_favorite).unwrap_or(false)
    } else {
        true
    };
    spawn(async move {
        for pid in &ids {
            let _ = rotero_db::papers::set_favorite(db.conn(), pid, new_val).await;
        }
        lib_state.with_mut(|s| {
            for pid in &ids {
                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid.as_str())) {
                    p.status.is_favorite = new_val;
                }
            }
        });
    });
}

fn action_toggle_read_selected(mut lib_state: Signal<LibraryState>, db: Database) {
    let ids: Vec<String> = lib_state.read().selected_paper_ids.iter().cloned().collect();
    if ids.is_empty() {
        return;
    }
    let new_val = if ids.len() == 1 {
        !lib_state.read().papers.iter().find(|p| p.id.as_deref() == Some(ids[0].as_str())).map(|p| p.status.is_read).unwrap_or(false)
    } else {
        true
    };
    spawn(async move {
        for pid in &ids {
            let _ = rotero_db::papers::set_read(db.conn(), pid, new_val).await;
        }
        lib_state.with_mut(|s| {
            for pid in &ids {
                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid.as_str())) {
                    p.status.is_read = new_val;
                }
            }
        });
    });
}

/// Handles keyboard shortcuts (window-scoped via onkeydown) and native menu events.
#[component]
pub fn GlobalKeyHandler() -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let render_ch = use_context::<RenderChannel>();
    let config = use_context::<Signal<SyncConfig>>();
    let new_coll_editing = use_context::<Signal<Option<Option<String>>>>();
    let dpr_sig = use_context::<Signal<DevicePixelRatio>>();

    let update_state = use_context::<Signal<UpdateState>>();

    let db_menu = db.clone();
    let _ = use_muda_event_handler(move |event| match event.id().0.as_str() {
        "open-pdf" => action_open_pdf(tabs, lib_state, config, dpr_sig),
        "import-bibtex" => action_import_bibtex(db_menu.clone(), lib_state),
        "export-bibtex" => action_export_bibtex(lib_state),
        "close-tab" => action_close_tab(tabs, lib_state, render_ch, config, dpr_sig),
        "find" => action_find(lib_state, tabs),
        "show-library" => action_show_library(lib_state),
        "new-collection" => action_new_collection(lib_state, new_coll_editing),
        "check-updates" => action_check_updates(update_state),
        _ => {}
    });

    rsx! {}
}

/// Keyboard event handler called from Layout's root div onkeydown.
#[allow(clippy::too_many_arguments)]
pub fn handle_keydown(
    event: Event<KeyboardData>,
    show_settings: Signal<ShowSettings>,
    lib_state: Signal<LibraryState>,
    tabs: Signal<PdfTabManager>,
    db: Database,
    render_ch: RenderChannel,
    config: Signal<SyncConfig>,
    new_coll_editing: Signal<Option<Option<String>>>,
    undo_stack: Signal<UndoStack>,
    tools: Signal<ViewerToolState>,
    dpr_sig: Signal<DevicePixelRatio>,
) {
    let key = event.key();
    let modifiers = event.modifiers();
    let cmd = modifiers.meta() || modifiers.ctrl();
    let shift = modifiers.shift();

    let in_library = !matches!(lib_state.read().view, LibraryView::PdfViewer | LibraryView::Graph);

    if cmd && !shift {
        if let Key::Character(ref c) = key {
            match c.as_str() {
                "," => {
                    event.prevent_default();
                    action_open_settings(show_settings);
                }
                "o" => {
                    event.prevent_default();
                    action_open_pdf(tabs, lib_state, config, dpr_sig);
                }
                "i" => {
                    event.prevent_default();
                    action_import_bibtex(db.clone(), lib_state);
                }
                "e" => {
                    event.prevent_default();
                    action_export_bibtex(lib_state);
                }
                "f" => {
                    event.prevent_default();
                    action_find(lib_state, tabs);
                }
                "l" => {
                    event.prevent_default();
                    action_focus_library_search(lib_state);
                }
                "w" => {
                    event.prevent_default();
                    action_close_tab(tabs, lib_state, render_ch, config, dpr_sig);
                }
                "n" => {
                    event.prevent_default();
                    action_new_collection(lib_state, new_coll_editing);
                }
                "1" => {
                    event.prevent_default();
                    action_show_library(lib_state);
                }
                "[" => {
                    event.prevent_default();
                    action_prev_tab(tabs, lib_state);
                }
                "]" => {
                    event.prevent_default();
                    action_next_tab(tabs, lib_state);
                }
                "z" => {
                    event.prevent_default();
                    action_undo(db.clone(), tabs, undo_stack);
                }
                "a" if in_library => {
                    event.prevent_default();
                    action_select_all(lib_state);
                }
                _ => {}
            }
        }
    } else if cmd && shift {
        if let Key::Character(ref c) = key {
            match c.as_str() {
                "z" | "Z" => {
                    event.prevent_default();
                    action_redo(db.clone(), tabs, undo_stack);
                }
                "f" | "F" if in_library => {
                    event.prevent_default();
                    action_toggle_favorite_selected(lib_state, db.clone());
                }
                "u" | "U" if in_library => {
                    event.prevent_default();
                    action_toggle_read_selected(lib_state, db.clone());
                }
                _ => {}
            }
        }
    } else if key == Key::Escape {
        action_escape(show_settings, tabs, tools, lib_state);
    } else if !cmd && in_library {
        match key {
            Key::ArrowDown => {
                event.prevent_default();
                action_select_next(lib_state);
            }
            Key::ArrowUp => {
                event.prevent_default();
                action_select_prev(lib_state);
            }
            Key::Enter => {
                event.prevent_default();
                action_open_selected_pdf(lib_state, tabs, &db, &config, &dpr_sig);
            }
            Key::Backspace | Key::Delete => {
                event.prevent_default();
                action_delete_selected(lib_state);
            }
            _ => {}
        }
    }
}

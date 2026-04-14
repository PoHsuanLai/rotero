use dioxus::prelude::*;
use dioxus_elements::HasFileData;

use crate::state::app_state::{LibraryState, LibraryView, SortField};
use crate::ui::chat_panel::ChatToggleButton;
use crate::ui::import_export::ImportExportButtons;
use crate::ui::search_bar::SearchBar;
use rotero_db::Database;

use super::add_paper::{AddPaperButtons, AddPaperDOIInput};
use super::duplicates::DuplicatesView;
use super::empty_state::LibraryEmptyState;
use super::external_results::ExternalResults;

#[component]
pub fn LibraryPanel() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut filtered_ids_signal = use_context_provider(|| Signal::new(Vec::<String>::new()));
    let db = use_context::<Database>();
    let render_ch = use_context::<crate::app::RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();
    // Derive just the view — only re-runs when view actually changes, not on every lib_state mutation
    let current_view = use_memo(move || lib_state.read().view.clone());
    // For saved searches, also extract the query so the effect doesn't read lib_state directly
    let saved_search_query = use_memo(move || {
        let state = lib_state.read();
        if let LibraryView::SavedSearch(ref search_id) = state.view {
            state
                .saved_searches
                .iter()
                .find(|s| s.id.as_deref() == Some(search_id.as_str()))
                .map(|s| s.query.clone())
        } else {
            None
        }
    });

    {
        let db_coll = db.clone();
        use_effect(move || {
            let view = current_view.read().clone();
            let search_query = saved_search_query.read().clone();
            match view {
                LibraryView::Collection(coll_id) => {
                    let db = db_coll.clone();
                    spawn(async move {
                        match rotero_db::collections::list_paper_ids_in_collection(
                            db.conn(),
                            &coll_id,
                        )
                        .await
                        {
                            Ok(ids) => {
                                lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                            }
                            Err(e) => tracing::error!("Failed to load collection papers: {e}"),
                        }
                    });
                }
                LibraryView::Tag(tag_id) => {
                    let db = db_coll.clone();
                    spawn(async move {
                        match rotero_db::tags::list_paper_ids_by_tag(db.conn(), &tag_id).await {
                            Ok(ids) => {
                                lib_state.with_mut(|s| s.filter.tag_paper_ids = Some(ids));
                            }
                            Err(e) => tracing::error!("Failed to load tag papers: {e}"),
                        }
                    });
                }
                LibraryView::Duplicates => {
                    let db = db_coll.clone();
                    spawn(async move {
                        match rotero_db::papers::find_duplicates(db.conn()).await {
                            Ok(groups) => {
                                lib_state.with_mut(|s| s.filter.duplicate_groups = Some(groups));
                            }
                            Err(e) => tracing::error!("Failed to find duplicates: {e}"),
                        }
                    });
                }
                LibraryView::SavedSearch(_) => {
                    if let Some(query) = search_query {
                        let db = db_coll.clone();
                        spawn(async move {
                            match rotero_db::papers::search_papers(db.conn(), &query).await {
                                Ok(papers) => {
                                    lib_state.with_mut(|s| s.search.results = Some(papers));
                                }
                                Err(e) => tracing::error!("Failed to run saved search: {e}"),
                            }
                        });
                    }
                }
                _ => {
                    lib_state.with_mut(|s| {
                        s.filter.collection_paper_ids = None;
                        s.filter.tag_paper_ids = None;
                        s.filter.duplicate_groups = None;
                    });
                }
            }
        });
    }

    let state = lib_state.read();

    let is_external = state.search.source != crate::state::app_state::SearchSource::Local;
    let external_results = state.search.external_results.clone();
    let external_searching = state.search.external_searching;
    let is_searching = state.search.results.is_some();

    let mut filtered: Vec<_> = if is_searching {
        state.search.results.as_ref().unwrap().clone()
    } else {
        match &state.view {
            LibraryView::AllPapers => state.papers.clone(),
            LibraryView::RecentlyAdded => state.papers.iter().take(20).cloned().collect(),
            LibraryView::Favorites => state
                .papers
                .iter()
                .filter(|p| p.status.is_favorite)
                .cloned()
                .collect(),
            LibraryView::Unread => state
                .papers
                .iter()
                .filter(|p| !p.status.is_read)
                .cloned()
                .collect(),
            LibraryView::Collection(_) => {
                if let Some(ref ids) = state.filter.collection_paper_ids {
                    state
                        .papers
                        .iter()
                        .filter(|p| p.id.as_ref().is_some_and(|pid| ids.contains(pid)))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            LibraryView::Tag(_) => {
                if let Some(ref ids) = state.filter.tag_paper_ids {
                    state
                        .papers
                        .iter()
                        .filter(|p| p.id.as_ref().is_some_and(|pid| ids.contains(pid)))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            LibraryView::Duplicates => {
                if let Some(ref groups) = state.filter.duplicate_groups {
                    groups.iter().flatten().cloned().collect()
                } else {
                    Vec::new()
                }
            }
            _ => state.papers.clone(),
        }
    };

    if !is_searching && !matches!(state.view, LibraryView::Duplicates) {
        state.sort_papers(&mut filtered);
    }

    // Provide ordered IDs to child components for shift-click range selection
    let ids: Vec<String> = filtered.iter().filter_map(|p| p.id.clone()).collect();
    filtered_ids_signal.set(ids);

    let duplicate_groups = if matches!(state.view, LibraryView::Duplicates) {
        state.filter.duplicate_groups.clone()
    } else {
        None
    };

    let view_title: String = if is_searching {
        "Search Results".to_string()
    } else {
        match &state.view {
            LibraryView::AllPapers => "All Papers".to_string(),
            LibraryView::RecentlyAdded => "Recently Added".to_string(),
            LibraryView::Favorites => "Favorites".to_string(),
            LibraryView::Unread => "Unread".to_string(),
            LibraryView::Collection(id) => state
                .collections
                .iter()
                .find(|c| c.id.as_deref() == Some(id.as_str()))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Collection".to_string()),
            LibraryView::Tag(id) => state
                .tags
                .iter()
                .find(|t| t.id.as_deref() == Some(id.as_str()))
                .map(|t| format!("Tag: {}", t.name))
                .unwrap_or_else(|| "Tag".to_string()),
            LibraryView::Duplicates => "Duplicates".to_string(),
            LibraryView::SavedSearch(id) => state
                .saved_searches
                .iter()
                .find(|s| s.id.as_deref() == Some(id.as_str()))
                .map(|s| format!("Search: {}", s.name))
                .unwrap_or_else(|| "Saved Search".to_string()),
            _ => "Papers".to_string(),
        }
    };

    let paper_count = filtered.len();

    let mut ctx_menu = use_signal(|| None::<(String, f64, f64)>);
    use_effect(|| {
        document::eval(
            r#"
            if (!window.__rotero_drag_ghost) {
                window.__rotero_drag_ghost = true;
                document.addEventListener('dragstart', function(e) {
                    let card = e.target.closest('.library-card, .sidebar-collection-item');
                    if (!card) return;
                    // Find the title text
                    let titleEl = card.querySelector('.library-card-title, .sidebar-collection-name');
                    let text = titleEl ? titleEl.textContent.trim() : 'Paper';
                    if (text.length > 40) text = text.substring(0, 37) + '...';
                    // Create compact ghost
                    let ghost = document.createElement('div');
                    ghost.textContent = text;
                    ghost.className = 'drag-ghost';
                    document.body.appendChild(ghost);
                    e.dataTransfer.setDragImage(ghost, 0, 0);
                    requestAnimationFrame(function() {
                        requestAnimationFrame(function() { ghost.remove(); });
                    });
                }, true);
            }
        "#,
        );
    });

    let _doi_show: Signal<bool> = use_context_provider(|| Signal::new(false));
    let _doi_err: Signal<Option<String>> = use_context_provider(|| Signal::new(None));

    let mut drag_over = use_signal(|| false);
    let drop_class = if drag_over() {
        "library-view library-view--dragover"
    } else {
        "library-view"
    };

    rsx! {
        div {
            class: "{drop_class}",
            ondragover: move |evt| {
                evt.prevent_default();
                drag_over.set(true);
            },
            ondragleave: move |_| {
                drag_over.set(false);
            },
            ondrop: move |evt| {
                drag_over.set(false);
                let dropped_files = evt.files();
                if !dropped_files.is_empty() {
                    let db = db.clone();
                    spawn(async move {
                        for file_data in &dropped_files {
                            let file_name = file_data.name();
                            let file_path = file_data.path();
                            let file_path_str = file_path.to_string_lossy().to_string();
                            if file_name.ends_with(".pdf") {
                                let title = std::path::Path::new(&file_name)
                                    .file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Untitled".to_string());

                                match db.import_pdf(&file_path_str, Some(&title), None, None) {
                                    Ok(rel_path) => {
                                        let mut paper = rotero_models::Paper {
                                            title,
                                            links: rotero_models::PaperLinks {
                                                pdf_path: Some(rel_path.clone()),
                                                ..Default::default()
                                            },
                                            ..Default::default()
                                        };
                                        let paper_id = match rotero_db::papers::insert_paper(db.conn(), &paper).await {
                                            Ok(id) => {
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
                                        let render_tx = render_ch.sender();
                                        let cfg = config.read();
                                        let data_dir = cfg.effective_library_path();
                                        let zoom = cfg.pdf.default_zoom * dpr_sig.read().0;
                                        drop(cfg);
                                        let db_for_cache = db.clone();
                                        let auto_fetch = config.read().auto_fetch_metadata;
                                        let meta_full_path = full_path.clone();
                                        let meta_render_tx = render_ch.sender();
                                        let meta_db = db.clone();
                                        let paper_id2 = paper_id.clone();
                                        spawn(async move {
                                            crate::state::commands::precache_pdf(&render_tx, &full_path, &data_dir, zoom, paper_id, Some(db_for_cache.conn())).await;
                                        });
                                        if let Some(pid) = paper_id2 {
                                            spawn(async move {
                                                crate::state::commands::extract_and_fetch_metadata(
                                                    &meta_render_tx, meta_db.conn(), &pid, &meta_full_path, auto_fetch, &mut lib_state,
                                                ).await;
                                            });
                                        }
                                    }
                                    Err(e) => tracing::error!("Failed to import {file_name}: {e}"),
                                }
                            }
                        }
                    });
                }
            },

            if drag_over() {
                div { class: "library-drop-overlay",
                    div { class: "library-drop-zone",
                        i { class: "library-drop-icon bi bi-archive" }
                        div { class: "library-drop-message", "Drop to import" }
                    }
                }
            }

            div { class: "library-header",
                div { class: "library-header-left",
                    h2 { class: "library-title", "{view_title}" }
                    span { class: "library-count", "{paper_count} papers" }
                }
                div { class: "library-header-right",
                    GraphToggleButton {}
                    ChatToggleButton {}
                    ImportExportButtons {}
                    AddPaperButtons {}
                }
            }
            AddPaperDOIInput {}

            div { class: "search-sort-row",
                SearchBar {}
                SortButton {}
            }

            if is_external {
                ExternalResults {
                    results: external_results.clone().unwrap_or_default(),
                    searching: external_searching,
                }
            }

            if !is_external {
            div { class: "library-list",
                if filtered.is_empty() {
                    LibraryEmptyState { view: state.view.clone(), is_searching }
                } else if let Some(ref groups) = duplicate_groups {
                    DuplicatesView { groups: groups.clone() }
                } else {
                    for paper in filtered.iter() {
                        {
                            let paper_id = paper.id.clone().unwrap_or_default();
                            let selected = state.is_selected(&paper_id);
                            rsx! {
                                super::paper_card::PaperCard {
                                    key: "{paper_id}",
                                    paper: paper.clone(),
                                    selected,
                                    ctx_menu,
                                }
                            }
                        }
                    }
                }
            }
            } // end if !is_external

            if let Some((_menu_paper_id, mx, my)) = ctx_menu() {
                {
                    let selected: Vec<String> = lib_state.read().selected_paper_ids.iter().cloned().collect();
                    if !selected.is_empty() {
                        rsx! {
                            super::context_menu::PaperContextMenu {
                                paper_ids: selected,
                                x: mx,
                                y: my,
                                on_close: move |_| ctx_menu.set(None),
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }

            // Delete confirmation dialog
            if lib_state.read().confirm_delete.is_some() {
                {
                    let delete_ids = lib_state.read().confirm_delete.clone().unwrap();
                    let count = delete_ids.len();
                    let message = if count == 1 { "Delete this paper?".to_string() } else { format!("Delete {count} papers?") };
                    let db_del = db.clone();
                    rsx! {
                        crate::ui::components::confirm_dialog::ConfirmDialog {
                            title: "Confirm Delete".to_string(),
                            message,
                            confirm_label: "Delete".to_string(),
                            danger: true,
                            on_confirm: move |_| {
                                let db = db_del.clone();
                                let ids = delete_ids.clone();
                                spawn(async move {
                                    for pid in &ids {
                                        let _ = rotero_db::papers::delete_paper(db.conn(), pid).await;
                                    }
                                    lib_state.with_mut(|s| {
                                        for pid in &ids {
                                            s.papers.retain(|p| p.id.as_deref() != Some(pid.as_str()));
                                            s.selected_paper_ids.remove(pid);
                                        }
                                        s.confirm_delete = None;
                                    });
                                });
                            },
                            on_cancel: move |_| {
                                lib_state.with_mut(|s| s.confirm_delete = None);
                            },
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GraphToggleButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let is_graph = lib_state.read().view == LibraryView::Graph;

    let class = if is_graph {
        "btn btn--ghost-active btn--sm"
    } else {
        "btn btn--ghost btn--sm"
    };

    rsx! {
        button {
            class,
            onclick: move |_| {
                lib_state.with_mut(|s| {
                    s.view = if s.view == LibraryView::Graph {
                        LibraryView::AllPapers
                    } else {
                        LibraryView::Graph
                    };
                });
            },
            i { class: "bi bi-diagram-2" }
            " Graph"
        }
    }
}

#[component]
fn SortButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let sort_field = lib_state.read().sort_field;
    let sort_ascending = lib_state.read().sort_ascending;
    let mut show_dropdown = use_signal(|| false);

    rsx! {
        div { class: "sort-wrapper",
            button {
                class: "btn btn--ghost btn--sm sort-btn",
                title: "Sort by {sort_field.label()}",
                onclick: move |_| show_dropdown.toggle(),
                i { class: "bi bi-arrow-down-up sort-icon" }
                span { class: "sort-label", "{sort_field.label()}" }
                i { class: "bi bi-chevron-down sort-chevron" }
            }
            button {
                class: "btn btn--ghost btn--sm sort-direction-btn",
                title: if sort_ascending { "Ascending" } else { "Descending" },
                onclick: move |_| {
                    lib_state.with_mut(|s| s.sort_ascending = !s.sort_ascending);
                },
                i { class: if sort_ascending { "bi bi-sort-up" } else { "bi bi-sort-down" } }
            }
            if show_dropdown() {
                div { class: "sort-dropdown",
                    for field in SortField::all().iter() {
                        button {
                            class: if *field == sort_field { "sort-option sort-option--active" } else { "sort-option" },
                            onclick: {
                                let f = *field;
                                move |_| {
                                    lib_state.with_mut(|s| {
                                        if s.sort_field == f {
                                            s.sort_ascending = !s.sort_ascending;
                                        } else {
                                            s.sort_field = f;
                                            s.sort_ascending = f.default_ascending();
                                        }
                                    });
                                    show_dropdown.set(false);
                                }
                            },
                            if *field == sort_field {
                                i {
                                    class: if sort_ascending { "bi bi-sort-up sort-option-icon" } else { "bi bi-sort-down sort-option-icon" },
                                }
                            }
                            "{field.label()}"
                        }
                    }
                }
                div {
                    class: "sort-backdrop",
                    onclick: move |_| show_dropdown.set(false),
                }
            }
        }
    }
}

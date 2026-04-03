use dioxus::prelude::*;
use dioxus_elements::HasFileData;

use super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use super::import_export::ImportExportButtons;
use super::search_bar::SearchBar;
use crate::db::Database;
use crate::state::app_state::{DragPaper, LibraryState, LibraryView, PdfTab, PdfTabManager};

#[component]
pub fn LibraryPanel() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let render_ch = use_context::<crate::app::RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    // Load collection/tag paper IDs when switching views
    {
        let db_coll = db.clone();
        use_effect(move || {
            let view = lib_state.read().view.clone();
            match view {
                LibraryView::Collection(coll_id) => {
                    let db = db_coll.clone();
                    spawn(async move {
                        match crate::db::collections::list_paper_ids_in_collection(
                            db.conn(),
                            coll_id,
                        )
                        .await
                        {
                            Ok(ids) => {
                                lib_state.with_mut(|s| s.collection_paper_ids = Some(ids));
                            }
                            Err(e) => eprintln!("Failed to load collection papers: {e}"),
                        }
                    });
                }
                LibraryView::Tag(tag_id) => {
                    let db = db_coll.clone();
                    spawn(async move {
                        match crate::db::tags::list_paper_ids_by_tag(db.conn(), tag_id).await {
                            Ok(ids) => {
                                lib_state.with_mut(|s| s.tag_paper_ids = Some(ids));
                            }
                            Err(e) => eprintln!("Failed to load tag papers: {e}"),
                        }
                    });
                }
                LibraryView::Duplicates => {
                    let db = db_coll.clone();
                    spawn(async move {
                        match crate::db::papers::find_duplicates(db.conn()).await {
                            Ok(groups) => {
                                lib_state.with_mut(|s| s.duplicate_groups = Some(groups));
                            }
                            Err(e) => eprintln!("Failed to find duplicates: {e}"),
                        }
                    });
                }
                LibraryView::SavedSearch(search_id) => {
                    // Find the query for this saved search
                    let query = lib_state
                        .read()
                        .saved_searches
                        .iter()
                        .find(|s| s.id == Some(search_id))
                        .map(|s| s.query.clone());
                    if let Some(query) = query {
                        let db = db_coll.clone();
                        spawn(async move {
                            match crate::db::papers::search_papers(db.conn(), &query).await {
                                Ok(papers) => {
                                    lib_state.with_mut(|s| s.search_results = Some(papers));
                                }
                                Err(e) => eprintln!("Failed to run saved search: {e}"),
                            }
                        });
                    }
                }
                _ => {
                    lib_state.with_mut(|s| {
                        s.collection_paper_ids = None;
                        s.tag_paper_ids = None;
                        s.duplicate_groups = None;
                    });
                }
            }
        });
    }

    let state = lib_state.read();

    let is_external = state.search_source != crate::state::app_state::SearchSource::Local;
    let external_results = state.external_results.clone();
    let external_searching = state.external_searching;
    let is_searching = state.search_results.is_some();

    let filtered: Vec<_> = if is_searching {
        state.search_results.as_ref().unwrap().clone()
    } else {
        match &state.view {
            LibraryView::AllPapers => state.papers.clone(),
            LibraryView::RecentlyAdded => {
                // Papers are already ordered by date_added DESC from the DB,
                // so just take the first 20 instead of cloning all + sorting.
                state.papers.iter().take(20).cloned().collect()
            }
            LibraryView::Favorites => state
                .papers
                .iter()
                .filter(|p| p.is_favorite)
                .cloned()
                .collect(),
            LibraryView::Unread => state
                .papers
                .iter()
                .filter(|p| !p.is_read)
                .cloned()
                .collect(),
            LibraryView::Collection(_) => {
                if let Some(ref ids) = state.collection_paper_ids {
                    state
                        .papers
                        .iter()
                        .filter(|p| p.id.is_some_and(|pid| ids.contains(&pid)))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            LibraryView::Tag(_) => {
                if let Some(ref ids) = state.tag_paper_ids {
                    state
                        .papers
                        .iter()
                        .filter(|p| p.id.is_some_and(|pid| ids.contains(&pid)))
                        .cloned()
                        .collect()
                } else {
                    Vec::new()
                }
            }
            LibraryView::Duplicates => {
                if let Some(ref groups) = state.duplicate_groups {
                    groups.iter().flatten().cloned().collect()
                } else {
                    Vec::new()
                }
            }
            _ => state.papers.clone(),
        }
    };

    // Duplicate groups for merge UI
    let duplicate_groups = if matches!(state.view, LibraryView::Duplicates) {
        state.duplicate_groups.clone()
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
                .find(|c| c.id == Some(*id))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Collection".to_string()),
            LibraryView::Tag(id) => state
                .tags
                .iter()
                .find(|t| t.id == Some(*id))
                .map(|t| format!("Tag: {}", t.name))
                .unwrap_or_else(|| "Tag".to_string()),
            LibraryView::Duplicates => "Duplicates".to_string(),
            LibraryView::SavedSearch(id) => state
                .saved_searches
                .iter()
                .find(|s| s.id == Some(*id))
                .map(|s| format!("Search: {}", s.name))
                .unwrap_or_else(|| "Saved Search".to_string()),
            _ => "Papers".to_string(),
        }
    };

    let paper_count = filtered.len();

    // Context menu state: (paper_id, x, y)
    let mut ctx_menu = use_signal(|| None::<(i64, f64, f64)>);

    // Paper drag state (shared with sidebar for drop onto collections/tags)
    let mut drag_paper = use_context::<Signal<DragPaper>>();

    // Install compact drag ghost for library cards and sidebar items
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

    // Drag and drop state
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
                                        let mut paper = rotero_models::Paper::new(title);
                                        paper.pdf_path = Some(rel_path.clone());
                                        let paper_id = match crate::db::papers::insert_paper(db.conn(), &paper).await {
                                            Ok(id) => {
                                                paper.id = Some(id);
                                                lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                Some(id)
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to insert paper: {e}");
                                                None
                                            }
                                        };
                                        // Pre-cache in background
                                        let full_path = db.resolve_pdf_path(&rel_path).to_string_lossy().to_string();
                                        let render_tx = render_ch.sender();
                                        let cfg = config.read();
                                        let data_dir = cfg.effective_library_path();
                                        let zoom = cfg.default_zoom;
                                        let quality = cfg.render_quality;
                                        drop(cfg);
                                        let db_for_cache = db.clone();
                                        let auto_fetch = config.read().auto_fetch_metadata;
                                        let meta_full_path = full_path.clone();
                                        let meta_render_tx = render_ch.sender();
                                        let meta_db = db.clone();
                                        spawn(async move {
                                            crate::state::commands::precache_pdf(&render_tx, &full_path, &data_dir, zoom, quality, paper_id, Some(db_for_cache.conn())).await;
                                        });
                                        // Extract metadata (DOI + CrossRef) in background
                                        if let Some(pid) = paper_id {
                                            spawn(async move {
                                                crate::state::commands::extract_and_fetch_metadata(
                                                    &meta_render_tx, meta_db.conn(), pid, &meta_full_path, auto_fetch, &mut lib_state,
                                                ).await;
                                            });
                                        }
                                    }
                                    Err(e) => eprintln!("Failed to import {file_name}: {e}"),
                                }
                            }
                        }
                    });
                }
            },

            // Drop zone overlay
            if drag_over() {
                div { class: "library-drop-overlay",
                    div { class: "library-drop-message",
                        "Drop PDF files here to import"
                    }
                }
            }

            // Header
            div { class: "library-header",
                div { class: "library-header-left",
                    h2 { class: "library-title", "{view_title}" }
                    span { class: "library-count", "{paper_count} papers" }
                }
                div { class: "library-header-right",
                    ImportExportButtons {}
                    AddPaperButton {}
                }
            }

            // Search bar
            SearchBar {}

            // External search results (when searching OpenAlex/arXiv/Semantic Scholar)
            if is_external {
                ExternalResults {
                    results: external_results.clone().unwrap_or_default(),
                    searching: external_searching,
                }
            }

            // Paper list (hidden when external search is active)
            if !is_external {
            div { class: "library-list",
                if filtered.is_empty() {
                    div { class: "library-empty",
                        if is_searching {
                            p { class: "library-empty-heading", "No results found" }
                            p { class: "library-empty-sub", "Try a different search term." }
                        } else if matches!(state.view, LibraryView::Collection(_)) {
                            p { class: "library-empty-heading", "No papers in this collection" }
                            p { class: "library-empty-sub", "Drag papers from the library to add them." }
                        } else if matches!(state.view, LibraryView::Tag(_)) {
                            p { class: "library-empty-heading", "No papers with this tag" }
                            p { class: "library-empty-sub", "Drag papers onto a tag in the sidebar to assign them." }
                        } else if matches!(state.view, LibraryView::Favorites) {
                            p { class: "library-empty-heading", "No favorites" }
                            p { class: "library-empty-sub", "Right-click a paper and select Favorite to add it here." }
                        } else if matches!(state.view, LibraryView::Unread) {
                            p { class: "library-empty-heading", "All caught up" }
                            p { class: "library-empty-sub", "No unread papers." }
                        } else if matches!(state.view, LibraryView::Duplicates) {
                            p { class: "library-empty-heading", "No duplicates found" }
                            p { class: "library-empty-sub", "Your library has no duplicate papers." }
                        } else if matches!(state.view, LibraryView::RecentlyAdded) {
                            p { class: "library-empty-heading", "No papers yet" }
                            p { class: "library-empty-sub", "Use \"+ Add PDF\" or the browser connector to import papers." }
                        } else {
                            p { class: "library-empty-heading", "No papers yet" }
                            p { class: "library-empty-sub", "Use \"+ Add PDF\" or the browser connector to import papers." }
                        }
                    }
                } else if let Some(ref groups) = duplicate_groups {
                    // Duplicate groups with merge buttons
                    for (gi, group) in groups.iter().enumerate() {
                        {
                            let group_key = format!("dup-group-{gi}");
                            let reason = if group.len() >= 2 && group[0].doi.is_some() && group[0].doi == group[1].doi {
                                format!("Shared DOI: {}", group[0].doi.as_deref().unwrap_or(""))
                            } else {
                                "Similar title".to_string()
                            };
                            rsx! {
                                div { key: "{group_key}", class: "duplicate-group",
                                    div { class: "duplicate-group-header",
                                        span { class: "duplicate-group-reason", "{reason}" }
                                        span { class: "duplicate-group-count", "{group.len()} papers" }
                                    }
                                    for paper in group.iter() {
                                        {
                                            let pid = paper.id.unwrap_or(0);
                                            let title = paper.title.clone();
                                            let authors = if paper.authors.is_empty() {
                                                "Unknown".to_string()
                                            } else if paper.authors.len() <= 2 {
                                                paper.authors.join(", ")
                                            } else {
                                                format!("{} et al.", paper.authors[0])
                                            };
                                            let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                                            let has_pdf = paper.pdf_path.is_some();
                                            let field_count = [
                                                paper.doi.is_some(),
                                                paper.abstract_text.is_some(),
                                                paper.journal.is_some(),
                                                paper.year.is_some(),
                                                paper.pdf_path.is_some(),
                                                !paper.authors.is_empty(),
                                            ].iter().filter(|&&b| b).count();
                                            rsx! {
                                                div { class: "duplicate-item",
                                                    div { class: "duplicate-item-info",
                                                        div { class: "library-card-title", "{title}" }
                                                        div { class: "library-card-meta",
                                                            span { class: "library-card-authors", "{authors}" }
                                                            if !year.is_empty() {
                                                                span { class: "library-card-sep", "\u{00b7}" }
                                                                span { class: "library-card-year", "{year}" }
                                                            }
                                                            if has_pdf {
                                                                span { class: "library-card-sep", "\u{00b7}" }
                                                                span { "PDF" }
                                                            }
                                                            span { class: "library-card-sep", "\u{00b7}" }
                                                            span { class: "duplicate-field-count", "{field_count}/6 fields" }
                                                        }
                                                    }
                                                    div { class: "duplicate-item-actions",
                                                        button {
                                                            class: "btn btn--sm btn--primary",
                                                            title: "Keep this paper and merge others into it",
                                                            onclick: {
                                                                let db = db.clone();
                                                                let other_ids: Vec<i64> = group.iter()
                                                                    .filter_map(|p| p.id)
                                                                    .filter(|&id| id != pid)
                                                                    .collect();
                                                                move |_| {
                                                                    let db = db.clone();
                                                                    let other_ids = other_ids.clone();
                                                                    spawn(async move {
                                                                        for del_id in &other_ids {
                                                                            let _ = crate::db::papers::merge_papers(db.conn(), pid, *del_id).await;
                                                                        }
                                                                        // Reload library
                                                                        if let Ok(papers) = crate::db::papers::list_papers(db.conn()).await {
                                                                            lib_state.with_mut(|s| {
                                                                                s.papers = papers;
                                                                                s.duplicate_groups = None;
                                                                            });
                                                                        }
                                                                        // Re-scan duplicates
                                                                        if let Ok(groups) = crate::db::papers::find_duplicates(db.conn()).await {
                                                                            lib_state.with_mut(|s| s.duplicate_groups = Some(groups));
                                                                        }
                                                                    });
                                                                }
                                                            },
                                                            "Keep"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    for paper in filtered.iter() {
                        {
                            let paper_id = paper.id.unwrap_or(0);
                            let title = paper.title.clone();
                            let pdf_rel_path = paper.pdf_path.clone();
                            let authors = if paper.authors.is_empty() {
                                "Unknown".to_string()
                            } else if paper.authors.len() <= 2 {
                                paper.authors.join(", ")
                            } else {
                                format!("{} et al.", paper.authors[0])
                            };
                            let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                            let journal = paper.journal.clone().unwrap_or_default();
                            let citation_count = paper.citation_count;
                            let has_pdf = paper.pdf_path.is_some();
                            let is_read = paper.is_read;
                            let is_fav = paper.is_favorite;
                            let selected = state.selected_paper_id == Some(paper_id);
                            let row_class = if selected {
                                "library-card library-card--selected"
                            } else {
                                "library-card"
                            };
                            let db_for_view = db.clone();
                            let db_for_fav = db.clone();

                            rsx! {
                                div {
                                    key: "{paper_id}",
                                    class: "{row_class}",
                                    draggable: "true",
                                    ondragstart: move |_| {
                                        drag_paper.set(DragPaper(Some(paper_id)));
                                    },
                                    ondragend: move |evt: Event<DragData>| {
                                        // Delay clearing so ondrop on the target fires first
                                        // drop_effect is "none" when dropped outside any valid target
                                        let _ = evt;
                                        spawn(async move {
                                            drag_paper.set(DragPaper(None));
                                        });
                                    },
                                    onmouseup: move |evt: Event<MouseData>| {
                                        // Only select if this wasn't a drag operation
                                        if drag_paper().0.is_none()
                                            && evt.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                                                lib_state.with_mut(|s| {
                                                    s.selected_paper_id = Some(paper_id);
                                                });
                                            }
                                    },
                                    oncontextmenu: move |evt| {
                                        evt.prevent_default();
                                        let coords = evt.page_coordinates();
                                        ctx_menu.set(Some((paper_id, coords.x, coords.y)));
                                    },

                                    // Left: read indicator
                                    div { class: "library-card-indicator",
                                        if !is_read {
                                            div { class: "library-unread-dot" }
                                        }
                                    }

                                    // Center: paper info
                                    div { class: "library-card-body",
                                        div { class: "library-card-title", "{title}" }
                                        div { class: "library-card-meta",
                                            span { class: "library-card-authors", "{authors}" }
                                            if !year.is_empty() {
                                                span { class: "library-card-sep", "\u{00b7}" }
                                                span { class: "library-card-year", "{year}" }
                                            }
                                            if !journal.is_empty() {
                                                span { class: "library-card-sep", "\u{00b7}" }
                                                span { class: "library-card-journal", "{journal}" }
                                            }
                                            if let Some(count) = citation_count {
                                                span { class: "library-card-sep", "\u{00b7}" }
                                                span { class: "library-card-citations", title: "Citation count", "{count} cited" }
                                            }
                                        }
                                    }

                                    // Right: actions
                                    div { class: "library-card-actions",
                                        // Favorite toggle
                                        button {
                                            class: if is_fav { "library-fav-btn library-fav-btn--active" } else { "library-fav-btn" },
                                            title: if is_fav { "Unfavorite" } else { "Favorite" },
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                let db = db_for_fav.clone();
                                                let new_val = !is_fav;
                                                spawn(async move {
                                                    if let Ok(()) = crate::db::papers::set_favorite(db.conn(), paper_id, new_val).await {
                                                        lib_state.with_mut(|s| {
                                                            if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
                                                                p.is_favorite = new_val;
                                                            }
                                                        });
                                                    }
                                                });
                                            },
                                            i { class: if is_fav { "bi bi-star-fill" } else { "bi bi-star" } }
                                        }

                                        // View PDF
                                        if has_pdf {
                                            button {
                                                class: "btn btn--ghost",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if let Some(ref rel_path) = pdf_rel_path {
                                                        let full_path = db_for_view.resolve_pdf_path(rel_path);
                                                        let path_str = full_path.to_string_lossy().to_string();
                                                        // Create or switch to tab — PdfViewer handles rendering
                                                        tabs.with_mut(|m| {
                                                            if let Some(idx) = m.find_by_paper_id(paper_id) {
                                                                let tid = m.tabs[idx].id;
                                                                m.switch_to(tid);
                                                            } else {
                                                                let cfg = config.read();
                                                                let id = m.next_id();
                                                                let mut tab = PdfTab::new(id, path_str.clone(), title.clone(), cfg.default_zoom, cfg.page_batch_size);
                                                                tab.paper_id = Some(paper_id);
                                                                m.open_tab(tab);
                                                            }
                                                        });
                                                        lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                                    }
                                                },
                                                "Open"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            } // end if !is_external

            // Paper context menu
            if let Some((menu_paper_id, mx, my)) = ctx_menu() {
                {
                    let state = lib_state.read();
                    let menu_paper = state.papers.iter().find(|p| p.id == Some(menu_paper_id)).cloned();
                    drop(state);

                    if let Some(paper) = menu_paper {
                        let has_pdf = paper.pdf_path.is_some();
                        let is_fav = paper.is_favorite;
                        let is_read = paper.is_read;
                        let doi = paper.doi.clone();
                        let pdf_rel = paper.pdf_path.clone();
                        let pid = menu_paper_id;
                        let db_ctx = db.clone();
                        let db_fav = db.clone();
                        let db_read = db.clone();
                        let db_del = db.clone();

                        rsx! {
                            ContextMenu {
                                x: mx,
                                y: my,
                                on_close: move |_| ctx_menu.set(None),

                                if has_pdf {
                                    ContextMenuItem {
                                        label: "Open PDF".to_string(),
                                        icon: Some("bi-eye".to_string()),
                                        on_click: move |_| {
                                            if let Some(ref rel_path) = pdf_rel {
                                                let full_path = db_ctx.resolve_pdf_path(rel_path);
                                                let path_str = full_path.to_string_lossy().to_string();
                                                tabs.with_mut(|m| {
                                                    if let Some(idx) = m.find_by_paper_id(pid) {
                                                        let tid = m.tabs[idx].id;
                                                        m.switch_to(tid);
                                                    } else {
                                                        let cfg = config.read();
                                                        let id = m.next_id();
                                                        let mut tab = PdfTab::new(id, path_str.clone(), paper.title.clone(), cfg.default_zoom, cfg.page_batch_size);
                                                        tab.paper_id = Some(pid);
                                                        m.open_tab(tab);
                                                    }
                                                });
                                                lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                            }
                                        },
                                    }
                                }

                                ContextMenuItem {
                                    label: if is_fav { "Unfavorite".to_string() } else { "Favorite".to_string() },
                                    icon: Some(if is_fav { "bi-star-fill".to_string() } else { "bi-star".to_string() }),
                                    on_click: move |_| {
                                        let db = db_fav.clone();
                                        let new_val = !is_fav;
                                        spawn(async move {
                                            if let Ok(()) = crate::db::papers::set_favorite(db.conn(), pid, new_val).await {
                                                lib_state.with_mut(|s| {
                                                    if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(pid)) {
                                                        p.is_favorite = new_val;
                                                    }
                                                });
                                            }
                                        });
                                    },
                                }

                                ContextMenuItem {
                                    label: if is_read { "Mark as unread".to_string() } else { "Mark as read".to_string() },
                                    icon: Some(if is_read { "bi-book".to_string() } else { "bi-book-fill".to_string() }),
                                    on_click: move |_| {
                                        let db = db_read.clone();
                                        let new_val = !is_read;
                                        spawn(async move {
                                            if let Ok(()) = crate::db::papers::set_read(db.conn(), pid, new_val).await {
                                                lib_state.with_mut(|s| {
                                                    if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(pid)) {
                                                        p.is_read = new_val;
                                                    }
                                                });
                                            }
                                        });
                                    },
                                }

                                ContextMenuItem {
                                    label: "Add Tag".to_string(),
                                    icon: Some("bi-tag".to_string()),
                                    on_click: move |_| {
                                        lib_state.with_mut(|s| {
                                            s.selected_paper_id = Some(pid);
                                        });
                                        // Focus the tag editor input after the detail panel renders
                                        document::eval("setTimeout(() => { let el = document.getElementById('tag-editor-input'); if (el) el.focus(); }, 100)");
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
                                                    let js = format!("navigator.clipboard.writeText(`{}`)", doi_copy);
                                                    document::eval(&js);
                                                },
                                            }
                                        }
                                    }
                                }

                                // Remove from collection (only when viewing a collection)
                                if let LibraryView::Collection(coll_id) = lib_state.read().view {
                                    {
                                        let db_remove = db.clone();
                                        rsx! {
                                            ContextMenuItem {
                                                label: "Remove from Collection".to_string(),
                                                icon: Some("bi-folder-minus".to_string()),
                                                on_click: move |_| {
                                                    let db = db_remove.clone();
                                                    spawn(async move {
                                                        if let Ok(()) = crate::db::collections::remove_paper_from_collection(db.conn(), pid, coll_id).await
                                                            && let Ok(ids) = crate::db::collections::list_paper_ids_in_collection(db.conn(), coll_id).await {
                                                                lib_state.with_mut(|s| s.collection_paper_ids = Some(ids));
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
                                    on_click: move |_| {
                                        let db = db_del.clone();
                                        spawn(async move {
                                            if let Ok(()) = crate::db::papers::delete_paper(db.conn(), pid).await {
                                                lib_state.with_mut(|s| {
                                                    s.papers.retain(|p| p.id != Some(pid));
                                                    if s.selected_paper_id == Some(pid) {
                                                        s.selected_paper_id = None;
                                                    }
                                                });
                                            }
                                        });
                                    },
                                }
                            }
                        }
                    } else {
                        rsx! {}
                    }
                }
            }
        }
    }
}

#[component]
fn AddPaperButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<crate::db::Database>();
    let render_ch = use_context::<crate::app::RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let mut error_msg = use_signal(|| None::<String>);
    let mut show_doi_input = use_signal(|| false);
    let mut doi_value = use_signal(String::new);

    let db_for_pdf = db.clone();
    let db_for_doi = db.clone();

    rsx! {
        div { class: "add-paper-row",
            button {
                class: "btn btn--primary",
                onclick: move |_| {
                    let db_for_pdf = db_for_pdf.clone();
                    spawn(async move {
                        let file = super::pick_file_async(&["pdf"], "Add Paper PDF").await;

                        if let Some(path) = file {
                            let path_str = path.to_string_lossy().to_string();
                            let db = db_for_pdf;

                            let filename = path.file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Untitled".to_string());

                            match db.import_pdf(&path_str, Some(&filename), None, None) {
                                Ok(rel_path) => {
                                    let mut paper = rotero_models::Paper::new(filename);
                                    paper.pdf_path = Some(rel_path.clone());
                                    let full_path = db.resolve_pdf_path(&rel_path).to_string_lossy().to_string();
                                    let auto_fetch = config.read().auto_fetch_metadata;
                                    let meta_render_tx = render_ch.sender();
                                    let meta_db = db.clone();

                                    match crate::db::papers::insert_paper(db.conn(), &paper).await {
                                        Ok(id) => {
                                            paper.id = Some(id);
                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                            error_msg.set(None);
                                            // Extract metadata in background
                                            spawn(async move {
                                                crate::state::commands::extract_and_fetch_metadata(
                                                    &meta_render_tx, meta_db.conn(), id, &full_path, auto_fetch, &mut lib_state,
                                                ).await;
                                            });
                                        }
                                        Err(e) => error_msg.set(Some(format!("{e}"))),
                                    }
                                }
                                Err(e) => error_msg.set(Some(e)),
                            }
                        }
                    });
                },
                "+ Add PDF"
            }

            button {
                class: "btn btn--success",
                onclick: move |_| {
                    show_doi_input.set(!show_doi_input());
                },
                "+ DOI"
            }
        }

        if show_doi_input() {
            div { class: "doi-input-row",
                input {
                    class: "input doi-input",
                    r#type: "text",
                    placeholder: "Enter DOI (e.g. 10.1234/...)",
                    value: "{doi_value}",
                    oninput: move |evt| doi_value.set(evt.value()),
                }
                button {
                    class: "btn btn--success",
                    onclick: move |_| {
                        let doi = doi_value().trim().to_string();
                        if doi.is_empty() {
                            return;
                        }
                        let db = db_for_doi.clone();

                        spawn(async move {
                            match crate::metadata::crossref::fetch_by_doi(&doi).await {
                                Ok(meta) => {
                                    let paper = crate::metadata::parser::metadata_to_paper(meta);
                                    match crate::db::papers::insert_paper(db.conn(), &paper).await {
                                        Ok(id) => {
                                            let mut paper = paper;
                                            paper.id = Some(id);
                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                            show_doi_input.set(false);
                                            doi_value.set(String::new());
                                            error_msg.set(None);
                                        }
                                        Err(e) => error_msg.set(Some(format!("{e}"))),
                                    }
                                }
                                Err(e) => error_msg.set(Some(e)),
                            }
                        });
                    },
                    "Fetch"
                }
            }
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { class: "error-message",
                "{err}"
            }
        }
    }
}

#[component]
fn ExternalResults(results: Vec<rotero_models::Paper>, searching: bool) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let source_label = lib_state.read().search_source.label();

    // Collect DOIs already in the library for duplicate detection
    let existing_dois: std::collections::HashSet<String> = lib_state
        .read()
        .papers
        .iter()
        .filter_map(|p| p.doi.clone())
        .filter(|d| !d.is_empty())
        .collect();

    rsx! {
        div { class: "library-list",
            if searching {
                div { class: "external-search-status",
                    i { class: "bi bi-arrow-repeat external-spinner" }
                    "Searching {source_label}..."
                }
            } else if results.is_empty() {
                div { class: "library-empty",
                    if lib_state.read().search_query.is_empty() {
                        p { class: "library-empty-heading", "Search {source_label}" }
                        p { class: "library-empty-sub", "Type a query and press Enter to search." }
                    } else if lib_state.read().external_results.is_some() {
                        p { class: "library-empty-heading", "No results found" }
                        p { class: "library-empty-sub", "Try a different search term." }
                    } else {
                        p { class: "library-empty-heading", "Search {source_label}" }
                        p { class: "library-empty-sub", "Press Enter to search." }
                    }
                }
            } else {
                {
                    let importable_count = results.iter().filter(|p| {
                        p.doi.as_ref().is_none_or(|d| d.is_empty() || !existing_dois.contains(d))
                    }).count();
                    let all_imported = importable_count == 0;
                    let db_banner = db.clone();

                    rsx! {
                        // Banner
                        div { class: "external-results-banner",
                            span { "{results.len()} results from {source_label}" }
                            button {
                                class: "btn btn--sm btn--primary",
                                disabled: all_imported,
                                onclick: move |_| {
                                    let state = lib_state.read();
                                    let papers = state.external_results.clone().unwrap_or_default();
                                    let existing: std::collections::HashSet<String> = state.papers.iter()
                                        .filter_map(|p| p.doi.clone())
                                        .filter(|d| !d.is_empty())
                                        .collect();
                                    drop(state);
                                    let db = db_banner.clone();
                                    spawn(async move {
                                        let mut imported = 0;
                                        for paper in papers {
                                            // Skip papers with DOIs we already have
                                            if let Some(ref doi) = paper.doi {
                                                if !doi.is_empty() && existing.contains(doi) {
                                                    continue;
                                                }
                                            }
                                            if let Ok(id) = crate::db::papers::insert_paper(db.conn(), &paper).await {
                                                let mut paper = paper;
                                                paper.id = Some(id);
                                                lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                imported += 1;
                                            }
                                        }
                                        eprintln!("Imported {imported} papers");
                                    });
                                },
                                if all_imported { "All Imported" } else { "Import All" }
                            }
                        }
                    }
                }
                for (i, paper) in results.iter().enumerate() {
                    {
                        let title = paper.title.clone();
                        let authors = if paper.authors.is_empty() {
                            "Unknown".to_string()
                        } else if paper.authors.len() <= 2 {
                            paper.authors.join(", ")
                        } else {
                            format!("{} et al.", paper.authors[0])
                        };
                        let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                        let journal = paper.journal.clone().unwrap_or_default();
                        let citation_count = paper.citation_count;
                        let doi = paper.doi.clone().unwrap_or_default();
                        let abstract_text = paper.abstract_text.clone().unwrap_or_default();
                        let has_abstract = !abstract_text.is_empty();
                        let already_imported = !doi.is_empty() && existing_dois.contains(&doi);
                        let paper_clone = paper.clone();
                        let db_import = db.clone();

                        rsx! {
                            div {
                                key: "ext-{i}",
                                class: if already_imported { "library-card external-result-card external-result-card--imported" } else { "library-card external-result-card" },

                                div { class: "library-card-body",
                                    div { class: "library-card-title", "{title}" }
                                    div { class: "library-card-meta",
                                        span { class: "library-card-authors", "{authors}" }
                                        if !year.is_empty() {
                                            span { class: "library-card-sep", "\u{00b7}" }
                                            span { class: "library-card-year", "{year}" }
                                        }
                                        if !journal.is_empty() {
                                            span { class: "library-card-sep", "\u{00b7}" }
                                            span { class: "library-card-journal", "{journal}" }
                                        }
                                        if let Some(count) = citation_count {
                                            span { class: "library-card-sep", "\u{00b7}" }
                                            span { class: "library-card-citations", title: "Citation count", "{count} cited" }
                                        }
                                        if !doi.is_empty() {
                                            span { class: "library-card-sep", "\u{00b7}" }
                                            span { class: "library-card-doi", "{doi}" }
                                        }
                                    }
                                    if has_abstract {
                                        div { class: "external-result-abstract", "{abstract_text}" }
                                    }
                                }

                                div { class: "library-card-actions",
                                    if already_imported {
                                        button {
                                            class: "btn btn--sm btn--ghost external-imported-btn",
                                            disabled: true,
                                            i { class: "bi bi-check-lg" }
                                            "Imported"
                                        }
                                    } else {
                                        button {
                                            class: "btn btn--sm btn--primary",
                                            onclick: move |_| {
                                                let paper = paper_clone.clone();
                                                let db = db_import.clone();
                                                spawn(async move {
                                                    // If we have a DOI but sparse metadata (autocomplete result),
                                                    // fetch full details first
                                                    let paper = enrich_before_import(paper).await;
                                                    match crate::db::papers::insert_paper(db.conn(), &paper).await {
                                                        Ok(id) => {
                                                            let mut paper = paper;
                                                            paper.id = Some(id);
                                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                                        }
                                                        Err(e) => eprintln!("Failed to import: {e}"),
                                                    }
                                                });
                                            },
                                            "Import"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// If a paper has a DOI but missing authors (e.g. from autocomplete),
/// try to fetch full metadata before inserting.
async fn enrich_before_import(paper: rotero_models::Paper) -> rotero_models::Paper {
    // Only enrich if we have a DOI but are missing basic fields
    let needs_enrichment =
        paper.authors.is_empty() && paper.doi.as_ref().is_some_and(|d| !d.is_empty());
    if !needs_enrichment {
        return paper;
    }

    let doi = paper.doi.as_deref().unwrap_or_default();
    // Try OpenAlex full endpoint first (fastest), then CrossRef
    if let Ok(meta) = crate::metadata::openalex::fetch_by_doi(doi).await {
        return crate::metadata::parser::metadata_to_paper(meta);
    }
    if let Ok(meta) = crate::metadata::crossref::fetch_by_doi(doi).await {
        return crate::metadata::parser::metadata_to_paper(meta);
    }
    paper
}

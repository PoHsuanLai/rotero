use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{DragPaper, LibraryState, LibraryView, PdfTab, PdfTabManager};
use rotero_models::Collection;
use super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};

#[component]
pub fn Sidebar(on_collapse: EventHandler<()>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let state = lib_state.read();
    let view = state.view.clone();
    let papers = &state.papers;

    // Compute counts for smart filters
    let total = papers.len();
    let favorites_count = papers.iter().filter(|p| p.is_favorite).count();
    let unread_count = papers.iter().filter(|p| !p.is_read).count();
    let recent_count = total.min(20);

    // Recently opened (last 5 papers viewed — tracked by date_modified)
    let mut recent_papers: Vec<_> = papers.iter()
        .filter(|p| p.pdf_path.is_some())
        .collect();
    recent_papers.sort_by(|a, b| b.date_modified.cmp(&a.date_modified));
    let recent_opened: Vec<_> = recent_papers.into_iter().take(5).collect();

    // Collection context menu state: (collection_id, name, x, y)
    let mut coll_ctx = use_signal(|| None::<(i64, String, f64, f64)>);

    // Tag context menu state: (tag_id, tag_name, tag_color, x, y)
    let mut tag_ctx = use_signal(|| None::<(i64, String, Option<String>, f64, f64)>);

    // Recently opened context menu state: (paper_id, x, y)
    let mut recent_ctx = use_signal(|| None::<(i64, f64, f64)>);

    // New collection inline edit state: None = not editing, Some(parent_id) = creating under parent
    // Some(None) = top-level, Some(Some(id)) = subcollection
    let new_coll_editing: Signal<Option<Option<i64>>> = use_context();

    // Drag-and-drop state for collection reparenting
    let mut drag_coll: Signal<Option<i64>> = use_context_provider(|| Signal::new(None::<i64>));

    // Drag paper from library
    let mut drag_paper = use_context::<Signal<DragPaper>>();

    // Track which drop target is being hovered during drag (e.g. "coll-5", "tag-3")
    let _drop_hover: Signal<Option<String>> = use_context_provider(|| Signal::new(None::<String>));

    let db_for_ctx = db.clone();

    rsx! {
        div { class: "sidebar",
            // Brand + collapse
            div { class: "sidebar-header",
                h2 {
                    class: "sidebar-brand",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                    },
                    "Rotero"
                }
                button {
                    class: "sidebar-collapse-btn",
                    onclick: move |_| on_collapse.call(()),
                    i { class: "bi bi-sidebar-collapse" }
                }
            }

            // Quick actions
            OpenPdfButton {}

            // Smart filters
            CollapsibleSection { title: "Library", initially_open: true,
                SidebarItem {
                    label: format!("All Papers"),
                    count: Some(total),
                    icon: "doc",
                    active: view == LibraryView::AllPapers,
                    view: LibraryView::AllPapers,
                }
                SidebarItem {
                    label: format!("Recently Added"),
                    count: Some(recent_count),
                    icon: "clock",
                    active: view == LibraryView::RecentlyAdded,
                    view: LibraryView::RecentlyAdded,
                }
                SidebarItem {
                    label: format!("Favorites"),
                    count: Some(favorites_count),
                    icon: "star",
                    active: view == LibraryView::Favorites,
                    view: LibraryView::Favorites,
                }
                SidebarItem {
                    label: format!("Unread"),
                    count: Some(unread_count),
                    icon: "circle",
                    active: view == LibraryView::Unread,
                    view: LibraryView::Unread,
                }
            }

            // Recently opened
            if !recent_opened.is_empty() {
                CollapsibleSection { title: "Recent", initially_open: true,
                    for paper in recent_opened.iter() {
                        {
                            let paper_id = paper.id.unwrap_or(0);
                            let title = paper.title.clone();
                            let pdf_rel = paper.pdf_path.clone();
                            let recent_icon = if paper.pdf_path.is_some() { "bi bi-file-earmark-pdf" } else { "bi bi-file-earmark-text" };
                            let db_recent = db.clone();
                            let truncated = if title.len() > 35 {
                                format!("{}...", &title[..32])
                            } else {
                                title.clone()
                            };
                            rsx! {
                                div {
                                    class: "sidebar-collection-item",
                                    draggable: "true",
                                    ondragstart: move |_| {
                                        drag_paper.set(DragPaper(Some(paper_id)));
                                    },
                                    ondragend: move |_| {
                                        spawn(async move {
                                            drag_paper.set(DragPaper(None));
                                        });
                                    },
                                    onmouseup: move |evt: Event<MouseData>| {
                                        if drag_paper().0.is_none() {
                                            if evt.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                                                if let Some(ref rel_path) = pdf_rel {
                                                    let full_path = db_recent.resolve_pdf_path(rel_path);
                                                    let path_str = full_path.to_string_lossy().to_string();
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
                                            }
                                        }
                                    },
                                    oncontextmenu: move |evt: Event<MouseData>| {
                                        evt.prevent_default();
                                        recent_ctx.set(Some((paper_id, evt.client_coordinates().x, evt.client_coordinates().y)));
                                    },
                                    i { class: "sidebar-collection-icon {recent_icon}" }
                                    span { class: "sidebar-collection-name", "{truncated}" }
                                }
                            }
                        }
                    }
                }
            }

            // Collections
            CollapsibleSection { title: "Collections", initially_open: true,
                action: rsx! { NewCollectionButton {} },
                CollectionTree { collections: state.collections.clone(), parent_id: None, depth: 0, ctx_menu: coll_ctx }
                if state.collections.is_empty() && new_coll_editing().is_none() {
                    p { class: "sidebar-empty", "No collections" }
                }
                // Inline new-collection row at top level
                if new_coll_editing() == Some(None) {
                    NewCollectionRow { parent_id: None, depth: 0 }
                }
                // Drop zone for un-nesting: drag a subcollection here to make it top-level
                if drag_coll().is_some() {
                    {
                        let db_unnest = db.clone();
                        rsx! {
                            div {
                                class: "sidebar-collection-item sidebar-collection-item--droptarget",
                                style: "justify-content: center; font-style: italic; opacity: 0.7;",
                                ondragover: move |evt| { evt.prevent_default(); },
                                ondrop: move |evt| {
                                    evt.prevent_default();
                                    if let Some(dragged_id) = drag_coll() {
                                        let db = db_unnest.clone();
                                        spawn(async move {
                                            if let Ok(()) = crate::db::collections::reparent_collection(db.conn(), dragged_id, None).await {
                                                lib_state.with_mut(|s| {
                                                    if let Some(c) = s.collections.iter_mut().find(|c| c.id == Some(dragged_id)) {
                                                        c.parent_id = None;
                                                    }
                                                });
                                            }
                                        });
                                        drag_coll.set(None);
                                    }
                                },
                                i { class: "sidebar-collection-icon bi bi-arrow-bar-left" }
                                span { class: "sidebar-collection-name", "Move to top level" }
                            }
                        }
                    }
                }
            }

            // Tags
            TagSection { tags: state.tags.clone(), ctx_menu: tag_ctx }

            // Spacer + Settings
            div { class: "sidebar-spacer" }
            super::settings::SettingsButton {}

            // Collection context menu
            if let Some((coll_id, coll_name, mx, my)) = coll_ctx() {
                {
                    let mut new_coll_editing: Signal<Option<Option<i64>>> = use_context();
                    let db_rename = db_for_ctx.clone();
                    let db_delete = db_for_ctx.clone();
                    let mut renaming = use_signal(|| false);
                    let mut rename_value = use_signal(|| coll_name.clone());

                    rsx! {
                        if renaming() {
                            // Inline rename input
                            ContextMenu {
                                x: mx,
                                y: my,
                                on_close: move |_| {
                                    renaming.set(false);
                                    coll_ctx.set(None);
                                },
                                div { class: "context-menu-rename",
                                    input {
                                        class: "input input--sm",
                                        r#type: "text",
                                        value: "{rename_value}",
                                        oninput: move |evt| rename_value.set(evt.value()),
                                        onkeypress: move |evt| {
                                            if evt.key() == Key::Enter {
                                                let new_name = rename_value().trim().to_string();
                                                if !new_name.is_empty() {
                                                    let db = db_rename.clone();
                                                    spawn(async move {
                                                        if let Ok(()) = crate::db::collections::rename_collection(db.conn(), coll_id, &new_name).await {
                                                            lib_state.with_mut(|s| {
                                                                if let Some(c) = s.collections.iter_mut().find(|c| c.id == Some(coll_id)) {
                                                                    c.name = new_name;
                                                                }
                                                            });
                                                        }
                                                        renaming.set(false);
                                                        coll_ctx.set(None);
                                                    });
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        } else {
                            ContextMenu {
                                x: mx,
                                y: my,
                                on_close: move |_| {
                                    coll_ctx.set(None);
                                },

                                ContextMenuItem {
                                    label: "New subcollection".to_string(),
                                    icon: Some("bi-folder-plus".to_string()),
                                    on_click: move |_| {
                                        new_coll_editing.set(Some(Some(coll_id)));
                                        coll_ctx.set(None);
                                    },
                                }

                                ContextMenuItem {
                                    label: "Rename".to_string(),
                                    icon: Some("bi-pencil".to_string()),
                                    on_click: move |_| {
                                        renaming.set(true);
                                    },
                                }

                                ContextMenuSeparator {}

                                ContextMenuItem {
                                    label: "Delete".to_string(),
                                    icon: Some("bi-trash".to_string()),
                                    danger: Some(true),
                                    on_click: move |_| {
                                        let db = db_delete.clone();
                                        spawn(async move {
                                            if let Ok(()) = crate::db::collections::delete_collection(db.conn(), coll_id).await {
                                                lib_state.with_mut(|s| {
                                                    s.collections.retain(|c| c.id != Some(coll_id));
                                                    if s.view == LibraryView::Collection(coll_id) {
                                                        s.view = LibraryView::AllPapers;
                                                    }
                                                });
                                            }
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }

            // Tag context menu
            if let Some((tag_id, tag_name, _tag_color, mx, my)) = tag_ctx() {
                {
                    let db_rename = db_for_ctx.clone();
                    let db_color = db_for_ctx.clone();
                    let db_delete = db_for_ctx.clone();
                    let mut renaming = use_signal(|| false);
                    let mut rename_value = use_signal(|| tag_name.clone());

                    let colors = vec![
                        ("#ffff00", "Yellow"),
                        ("#ff6b6b", "Red"),
                        ("#51cf66", "Green"),
                        ("#339af0", "Blue"),
                        ("#cc5de8", "Purple"),
                        ("#ff922b", "Orange"),
                    ];

                    rsx! {
                        if renaming() {
                            ContextMenu {
                                x: mx,
                                y: my,
                                on_close: move |_| {
                                    renaming.set(false);
                                    tag_ctx.set(None);
                                },
                                div { class: "context-menu-rename",
                                    input {
                                        class: "input input--sm",
                                        r#type: "text",
                                        value: "{rename_value}",
                                        oninput: move |evt| rename_value.set(evt.value()),
                                        onkeypress: move |evt| {
                                            if evt.key() == Key::Enter {
                                                let new_name = rename_value().trim().to_string();
                                                if !new_name.is_empty() {
                                                    let db = db_rename.clone();
                                                    spawn(async move {
                                                        if let Ok(()) = crate::db::tags::rename_tag(db.conn(), tag_id, &new_name).await {
                                                            lib_state.with_mut(|s| {
                                                                if let Some(t) = s.tags.iter_mut().find(|t| t.id == Some(tag_id)) {
                                                                    t.name = new_name;
                                                                }
                                                            });
                                                        }
                                                        renaming.set(false);
                                                        tag_ctx.set(None);
                                                    });
                                                }
                                            }
                                        },
                                    }
                                }
                            }
                        } else {
                            ContextMenu {
                                x: mx,
                                y: my,
                                on_close: move |_| {
                                    tag_ctx.set(None);
                                },

                                ContextMenuItem {
                                    label: "Filter by tag".to_string(),
                                    icon: Some("bi-funnel".to_string()),
                                    on_click: move |_| {
                                        lib_state.with_mut(|s| s.view = LibraryView::Tag(tag_id));
                                        tag_ctx.set(None);
                                    },
                                }

                                // Color swatches
                                div { class: "context-menu-item",
                                    i { class: "context-menu-icon bi bi-palette" }
                                    span { class: "context-menu-label", "Color" }
                                    div { class: "context-menu-colors",
                                        for (color, _label) in colors.iter() {
                                            {
                                                let color = color.to_string();
                                                let color_for_click = color.clone();
                                                let db_swatch = db_color.clone();
                                                rsx! {
                                                    span {
                                                        class: "context-menu-color-swatch",
                                                        style: "background: {color};",
                                                        onclick: move |evt| {
                                                            evt.stop_propagation();
                                                            let c = color_for_click.clone();
                                                            let db = db_swatch.clone();
                                                            spawn(async move {
                                                                if let Ok(()) = crate::db::tags::update_tag_color(db.conn(), tag_id, &c).await {
                                                                    lib_state.with_mut(|s| {
                                                                        if let Some(t) = s.tags.iter_mut().find(|t| t.id == Some(tag_id)) {
                                                                            t.color = Some(c);
                                                                        }
                                                                    });
                                                                }
                                                                tag_ctx.set(None);
                                                            });
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                ContextMenuItem {
                                    label: "Rename".to_string(),
                                    icon: Some("bi-pencil".to_string()),
                                    on_click: move |_| {
                                        renaming.set(true);
                                    },
                                }

                                ContextMenuSeparator {}

                                ContextMenuItem {
                                    label: "Delete".to_string(),
                                    icon: Some("bi-trash".to_string()),
                                    danger: Some(true),
                                    on_click: move |_| {
                                        let db = db_delete.clone();
                                        spawn(async move {
                                            if let Ok(()) = crate::db::tags::delete_tag(db.conn(), tag_id).await {
                                                lib_state.with_mut(|s| {
                                                    s.tags.retain(|t| t.id != Some(tag_id));
                                                    if s.view == LibraryView::Tag(tag_id) {
                                                        s.view = LibraryView::AllPapers;
                                                    }
                                                });
                                            }
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }

            // Recently opened context menu
            if let Some((paper_id, mx, my)) = recent_ctx() {
                ContextMenu {
                    x: mx,
                    y: my,
                    on_close: move |_| {
                        recent_ctx.set(None);
                    },

                    ContextMenuItem {
                        label: "Show in library".to_string(),
                        icon: Some("bi-collection".to_string()),
                        on_click: move |_| {
                            lib_state.with_mut(|s| {
                                s.view = LibraryView::AllPapers;
                                s.selected_paper_id = Some(paper_id);
                            });
                            recent_ctx.set(None);
                        },
                    }
                }
            }
        }
    }
}

/// A single sidebar navigation item with icon, label, and optional count.
#[component]
fn SidebarItem(label: String, count: Option<usize>, icon: String, active: bool, view: LibraryView) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let class = if active {
        "sidebar-nav-item sidebar-nav-item--active"
    } else {
        "sidebar-nav-item"
    };

    let icon_class = match icon.as_str() {
        "doc" => "bi bi-journal-text",
        "clock" => "bi bi-clock",
        "star" => "bi bi-star",
        "circle" => "bi bi-circle",
        _ => "",
    };

    rsx! {
        div {
            class: "{class}",
            onclick: move |_| {
                lib_state.with_mut(|s| s.view = view.clone());
            },
            i { class: "sidebar-nav-icon {icon_class}" }
            span { class: "sidebar-nav-label", "{label}" }
            if let Some(n) = count {
                span { class: "sidebar-nav-count", "{n}" }
            }
        }
    }
}

/// A collapsible section with a header and children.
#[component]
fn CollapsibleSection(title: String, initially_open: Option<bool>, action: Option<Element>, children: Element) -> Element {
    let mut open = use_signal(|| initially_open.unwrap_or(true));

    let arrow_class = if open() { "bi bi-chevron-down" } else { "bi bi-chevron-right" };

    rsx! {
        div { class: "sidebar-section",
            div { class: "sidebar-section-header",
                div {
                    class: "sidebar-section-toggle",
                    onclick: move |_| open.set(!open()),
                    i { class: "sidebar-section-arrow {arrow_class}" }
                    h3 { class: "sidebar-section-title", "{title}" }
                }
                if let Some(action_el) = action {
                    {action_el}
                }
            }
            if open() {
                div { class: "sidebar-section-content",
                    {children}
                }
            }
        }
    }
}

/// Renders a nested collection tree recursively.
#[component]
fn CollectionTree(collections: Vec<Collection>, parent_id: Option<i64>, depth: u32, ctx_menu: Signal<Option<(i64, String, f64, f64)>>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let new_coll_editing = use_context::<Signal<Option<Option<i64>>>>();
    let mut drag_coll = use_context::<Signal<Option<i64>>>();
    let mut drag_paper = use_context::<Signal<DragPaper>>();
    let mut drop_hover = use_context::<Signal<Option<String>>>();
    let mut coll_ctx = ctx_menu;
    let lib = lib_state.read();
    let view = lib.view.clone();

    let children: Vec<_> = collections.iter()
        .filter(|c| c.parent_id == parent_id)
        .cloned()
        .collect();

    if children.is_empty() && new_coll_editing() != Some(parent_id) {
        return rsx! {};
    }

    let indent = depth * 16;

    rsx! {
        for coll in children.iter() {
            {
                let coll_id = coll.id.unwrap_or(0);
                let coll_name = coll.name.clone();
                let is_active = view == LibraryView::Collection(coll_id);
                let class = if is_active {
                    "sidebar-collection-item sidebar-collection-item--active"
                } else {
                    "sidebar-collection-item"
                };

                let has_children = collections.iter().any(|c| c.parent_id == Some(coll_id));
                let collections_clone = collections.clone();
                let creating_under_this = new_coll_editing() == Some(Some(coll_id));

                // Icon: open folder if active, filled folder if has children, outline if empty
                let folder_icon = if is_active {
                    "bi bi-folder2-open"
                } else if has_children {
                    "bi bi-folder-fill"
                } else {
                    "bi bi-folder"
                };

                let is_drag_active = (drag_coll().is_some() && drag_coll() != Some(coll_id)) || drag_paper().0.is_some();
                let is_hover = drop_hover().as_deref() == Some(&format!("coll-{coll_id}"));
                let item_class = if is_drag_active && is_hover {
                    format!("{class} sidebar-collection-item--drophover")
                } else if is_drag_active {
                    format!("{class} sidebar-collection-item--droptarget")
                } else {
                    class.to_string()
                };
                let db_for_drop = db.clone();
                let db_for_paper_drop = db.clone();

                rsx! {
                    div {
                        key: "coll-{coll_id}",
                        class: "{item_class}",
                        style: "padding-left: {indent + 8}px;",
                        draggable: "true",
                        onmouseup: move |evt: Event<MouseData>| {
                            // Only navigate if this wasn't a drag operation
                            if drag_coll().is_none() {
                                if evt.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                                    lib_state.with_mut(|s| s.view = LibraryView::Collection(coll_id));
                                }
                            }
                        },
                        oncontextmenu: {
                            let name = coll_name.clone();
                            move |evt: Event<MouseData>| {
                                evt.prevent_default();
                                let coords = evt.page_coordinates();
                                coll_ctx.set(Some((coll_id, name.clone(), coords.x, coords.y)));
                            }
                        },
                        ondragstart: move |_| {
                            drag_coll.set(Some(coll_id));
                        },
                        ondragenter: move |_| {
                            drop_hover.set(Some(format!("coll-{coll_id}")));
                        },
                        ondragleave: move |_| {
                            if drop_hover().as_deref() == Some(&format!("coll-{coll_id}")) {
                                drop_hover.set(None);
                            }
                        },
                        ondragover: move |evt| {
                            evt.prevent_default();
                        },
                        ondrop: move |evt| {
                            evt.prevent_default();
                            drop_hover.set(None);
                            if let Some(dragged_id) = drag_coll() {
                                if dragged_id != coll_id {
                                    // Reparent dragged collection under this one
                                    let db = db_for_drop.clone();
                                    spawn(async move {
                                        if let Ok(()) = crate::db::collections::reparent_collection(db.conn(), dragged_id, Some(coll_id)).await {
                                            lib_state.with_mut(|s| {
                                                if let Some(c) = s.collections.iter_mut().find(|c| c.id == Some(dragged_id)) {
                                                    c.parent_id = Some(coll_id);
                                                }
                                            });
                                        }
                                    });
                                }
                                drag_coll.set(None);
                            } else if let Some(paper_id) = drag_paper().0 {
                                // Add paper to this collection
                                let db = db_for_paper_drop.clone();
                                spawn(async move {
                                    if let Ok(()) = crate::db::collections::add_paper_to_collection(db.conn(), paper_id, coll_id).await {
                                        // Refresh only if currently viewing this collection
                                        let current_view = lib_state.read().view.clone();
                                        if current_view == LibraryView::Collection(coll_id) {
                                            if let Ok(ids) = crate::db::collections::list_paper_ids_in_collection(db.conn(), coll_id).await {
                                                lib_state.with_mut(|s| s.collection_paper_ids = Some(ids));
                                            }
                                        }
                                    }
                                });
                                drag_paper.set(DragPaper(None));
                            }
                        },
                        ondragend: move |_| {
                            drag_coll.set(None);
                        },
                        i { class: "sidebar-collection-icon {folder_icon}" }
                        span { class: "sidebar-collection-name", "{coll_name}" }
                    }
                    // Render children (recursive)
                    if has_children || creating_under_this {
                        CollectionTree {
                            collections: collections_clone,
                            parent_id: Some(coll_id),
                            depth: depth + 1,
                            ctx_menu: coll_ctx,
                        }
                    }
                    // Inline new subcollection row
                    if creating_under_this {
                        NewCollectionRow { parent_id: Some(coll_id), depth: depth + 1 }
                    }
                }
            }
        }
    }
}

/// The "+" button in the Collections section header.
/// If a collection is currently selected, creates under it; otherwise top-level.
#[component]
fn NewCollectionButton() -> Element {
    let mut editing = use_context::<Signal<Option<Option<i64>>>>();
    let lib_state = use_context::<Signal<LibraryState>>();

    rsx! {
        button {
            class: "sidebar-add-btn",
            onclick: move |_| {
                // If viewing a collection, create subcollection; otherwise top-level
                let parent = match lib_state.read().view {
                    LibraryView::Collection(id) => Some(id),
                    _ => None,
                };
                editing.set(Some(parent));
            },
            i { class: "bi bi-plus-lg" }
        }
    }
}

/// An inline editable row that looks like a regular collection item.
#[component]
fn NewCollectionRow(parent_id: Option<i64>, depth: u32) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut editing = use_context::<Signal<Option<Option<i64>>>>();
    let mut name_value = use_signal(|| String::new());
    let mut submitted = use_signal(|| false);

    let indent = depth * 16;

    rsx! {
        div {
            class: "sidebar-collection-item sidebar-collection-item--editing",
            style: "padding-left: {indent + 8}px;",
            i { class: "sidebar-collection-icon bi bi-folder" }
            input {
                class: "sidebar-inline-rename",
                r#type: "text",
                placeholder: "Collection name",
                value: "{name_value}",
                oninput: move |evt| name_value.set(evt.value()),
                onmounted: move |evt| { let _ = evt.set_focus(true); },
                onkeydown: move |evt| {
                    match evt.key() {
                        Key::Enter => {
                            let name = name_value().trim().to_string();
                            if !name.is_empty() {
                                submitted.set(true);
                                let mut coll = rotero_models::Collection::new(name);
                                coll.parent_id = parent_id;
                                let db = db.clone();
                                spawn(async move {
                                    if let Ok(id) = crate::db::collections::insert_collection(db.conn(), &coll).await {
                                        let mut coll = coll;
                                        coll.id = Some(id);
                                        lib_state.with_mut(|s| s.collections.push(coll));
                                    }
                                    editing.set(None);
                                    name_value.set(String::new());
                                });
                            } else {
                                editing.set(None);
                                name_value.set(String::new());
                            }
                        }
                        Key::Escape => {
                            editing.set(None);
                            name_value.set(String::new());
                        }
                        _ => {}
                    }
                },
                onfocusout: move |_| {
                    if !submitted() {
                        editing.set(None);
                        name_value.set(String::new());
                    }
                },
            }
        }
    }
}

/// Self-contained tag section with collapsibility and drag-drop support.
/// Needs to be its own component so signal reads are properly tracked
/// (CollapsibleSection children don't re-render when context signals change).
#[component]
fn TagSection(tags: Vec<rotero_models::Tag>, ctx_menu: Signal<Option<(i64, String, Option<String>, f64, f64)>>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut drag_paper = use_context::<Signal<DragPaper>>();
    let mut drop_hover = use_context::<Signal<Option<String>>>();
    let mut tag_ctx = ctx_menu;
    let mut open = use_signal(|| true);

    let arrow_class = if open() { "bi bi-chevron-down" } else { "bi bi-chevron-right" };

    rsx! {
        div { class: "sidebar-section",
            div { class: "sidebar-section-header",
                div {
                    class: "sidebar-section-toggle",
                    onclick: move |_| open.set(!open()),
                    i { class: "sidebar-section-arrow {arrow_class}" }
                    h3 { class: "sidebar-section-title", "Tags" }
                }
            }
            if open() {
                div { class: "sidebar-section-content",
                    if tags.is_empty() {
                        p { class: "sidebar-empty", "No tags" }
                    } else {
                        div { class: "sidebar-tags-wrap",
                            for tag in tags.iter() {
                                {
                                    let tag_id = tag.id.unwrap_or(0);
                                    let tag_name = tag.name.clone();
                                    let tag_color = tag.color.clone();
                                    let bg = tag_color.clone().unwrap_or_else(|| "#6b7085".to_string());
                                    let is_paper_drop = drag_paper.read().0.is_some();
                                    let is_hover = drop_hover().as_deref() == Some(&format!("tag-{tag_id}"));
                                    let tag_class = if is_paper_drop && is_hover {
                                        "sidebar-tag sidebar-tag--drophover"
                                    } else if is_paper_drop {
                                        "sidebar-tag sidebar-tag--droptarget"
                                    } else {
                                        "sidebar-tag"
                                    };
                                    let db_for_tag_drop = db.clone();
                                    rsx! {
                                        span {
                                            class: "{tag_class}",
                                            style: "background: {bg};",
                                            onclick: move |_| {
                                                lib_state.with_mut(|s| s.view = LibraryView::Tag(tag_id));
                                            },
                                            oncontextmenu: move |evt: Event<MouseData>| {
                                                evt.prevent_default();
                                                tag_ctx.set(Some((tag_id, tag_name.clone(), tag_color.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                                            },
                                            ondragenter: move |_| {
                                                drop_hover.set(Some(format!("tag-{tag_id}")));
                                            },
                                            ondragleave: move |_| {
                                                if drop_hover().as_deref() == Some(&format!("tag-{tag_id}")) {
                                                    drop_hover.set(None);
                                                }
                                            },
                                            ondragover: move |evt| {
                                                evt.prevent_default();
                                            },
                                            ondrop: move |evt| {
                                                evt.prevent_default();
                                                drop_hover.set(None);
                                                if let Some(paper_id) = drag_paper().0 {
                                                    let db = db_for_tag_drop.clone();
                                                    spawn(async move {
                                                        let _ = crate::db::tags::add_tag_to_paper(db.conn(), paper_id, tag_id).await;
                                                        let current_view = lib_state.read().view.clone();
                                                        if current_view == LibraryView::Tag(tag_id) {
                                                            if let Ok(ids) = crate::db::tags::list_paper_ids_by_tag(db.conn(), tag_id).await {
                                                                lib_state.with_mut(|s| s.tag_paper_ids = Some(ids));
                                                            }
                                                        }
                                                    });
                                                    drag_paper.set(DragPaper(None));
                                                }
                                            },
                                            i { class: "sidebar-tag-icon bi bi-tag" }
                                            "{tag.name}"
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

#[component]
fn OpenPdfButton() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let error_msg = use_signal(|| None::<String>);

    rsx! {
        button {
            class: "sidebar-open-btn",
            onclick: move |_| {
                let file = super::pick_file(&["pdf"], "Open PDF");

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
            },
            "Open PDF"
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { class: "sidebar-error", "{err}" }
        }
    }
}

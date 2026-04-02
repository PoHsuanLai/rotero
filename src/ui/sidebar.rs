use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfViewState};
use rotero_models::Collection;
use super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};

#[component]
pub fn Sidebar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
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

    // Collection context menu state: (collection_id, x, y)
    let mut coll_ctx = use_signal(|| None::<(i64, String, f64, f64)>);

    rsx! {
        div { class: "sidebar",
            // Brand
            h2 { class: "sidebar-brand", "Rotero" }

            // Quick actions
            OpenPdfButton {}

            // Smart filters
            div { class: "sidebar-section",
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
                            let title = paper.title.clone();
                            let truncated = if title.len() > 35 {
                                format!("{}...", &title[..32])
                            } else {
                                title
                            };
                            rsx! {
                                div { class: "sidebar-recent-item",
                                    "{truncated}"
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
                if state.collections.is_empty() {
                    p { class: "sidebar-empty", "No collections" }
                }
            }

            // Tags
            CollapsibleSection { title: "Tags", initially_open: true,
                if state.tags.is_empty() {
                    p { class: "sidebar-empty", "No tags" }
                } else {
                    div { class: "sidebar-tags-wrap",
                        for tag in state.tags.iter() {
                            {
                                let tag_name = tag.name.clone();
                                let bg = tag.color.clone().unwrap_or_else(|| "#6b7085".to_string());
                                rsx! {
                                    span {
                                        class: "sidebar-tag",
                                        style: "background: {bg};",
                                        "{tag_name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Spacer + Settings
            div { class: "sidebar-spacer" }
            super::settings::SettingsButton {}

            // Collection context menu
            if let Some((coll_id, coll_name, mx, my)) = coll_ctx() {
                {
                    let db_delete = db.clone();

                    rsx! {
                        ContextMenu {
                            x: mx,
                            y: my,
                            on_close: move |_| {
                                coll_ctx.set(None);
                            },

                            ContextMenuItem {
                                label: "Rename".to_string(),
                                icon: Some("bi-pencil".to_string()),
                                disabled: Some(true),
                                on_click: move |_| {},
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
    let mut coll_ctx = ctx_menu;
    let lib = lib_state.read();
    let view = lib.view.clone();

    let children: Vec<_> = collections.iter()
        .filter(|c| c.parent_id == parent_id)
        .cloned()
        .collect();

    if children.is_empty() {
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

                // Count papers in this collection (from paper_collections in state would be ideal,
                // but for now just show the collection)
                let has_children = collections.iter().any(|c| c.parent_id == Some(coll_id));
                let collections_clone = collections.clone();

                rsx! {
                    div {
                        key: "coll-{coll_id}",
                        class: "{class}",
                        style: "padding-left: {indent + 8}px;",
                        onclick: move |_| {
                            lib_state.with_mut(|s| s.view = LibraryView::Collection(coll_id));
                        },
                        oncontextmenu: {
                            let name = coll_name.clone();
                            move |evt: Event<MouseData>| {
                                evt.prevent_default();
                                let coords = evt.page_coordinates();
                                coll_ctx.set(Some((coll_id, name.clone(), coords.x, coords.y)));
                            }
                        },
                        i { class: "sidebar-collection-icon bi bi-folder" }
                        span { class: "sidebar-collection-name", "{coll_name}" }
                    }
                    if has_children {
                        CollectionTree {
                            collections: collections_clone,
                            parent_id: Some(coll_id),
                            depth: depth + 1,
                            ctx_menu: coll_ctx,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn NewCollectionButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut show_input = use_signal(|| false);
    let mut name_value = use_signal(|| String::new());

    let db_for_submit = db.clone();
    let db_for_key = db.clone();

    let do_submit = move |_| {
        let name = name_value().trim().to_string();
        if !name.is_empty() {
            let coll = rotero_models::Collection::new(name);
            let db = db_for_submit.clone();
            spawn(async move {
                if let Ok(id) = crate::db::collections::insert_collection(db.conn(), &coll).await {
                    let mut coll = coll;
                    coll.id = Some(id);
                    lib_state.with_mut(|s| s.collections.push(coll));
                }
            });
        }
        show_input.set(false);
        name_value.set(String::new());
    };

    let do_cancel = move |_: Event<MouseData>| {
        show_input.set(false);
        name_value.set(String::new());
    };

    rsx! {
        if show_input() {
            div { class: "sidebar-new-collection",
                div { class: "sidebar-new-collection-row",
                    i { class: "sidebar-collection-icon bi bi-folder" }
                    input {
                        class: "sidebar-inline-input",
                        r#type: "text",
                        placeholder: "New collection",
                        value: "{name_value}",
                        oninput: move |evt| name_value.set(evt.value()),
                        onkeydown: move |evt| {
                            match evt.key() {
                                Key::Enter => {
                                    let name = name_value().trim().to_string();
                                    if !name.is_empty() {
                                        let coll = rotero_models::Collection::new(name);
                                        let db = db_for_key.clone();
                                        spawn(async move {
                                            if let Ok(id) = crate::db::collections::insert_collection(db.conn(), &coll).await {
                                                let mut coll = coll;
                                                coll.id = Some(id);
                                                lib_state.with_mut(|s| s.collections.push(coll));
                                            }
                                        });
                                    }
                                    show_input.set(false);
                                    name_value.set(String::new());
                                }
                                Key::Escape => {
                                    show_input.set(false);
                                    name_value.set(String::new());
                                }
                                _ => {}
                            }
                        },
                    }
                }
                div { class: "sidebar-new-collection-actions",
                    button {
                        class: "sidebar-inline-btn sidebar-inline-btn--confirm",
                        onclick: do_submit,
                        i { class: "bi bi-check2" }
                    }
                    button {
                        class: "sidebar-inline-btn sidebar-inline-btn--cancel",
                        onclick: do_cancel,
                        "\u{00d7}"
                    }
                }
            }
        } else {
            button {
                class: "sidebar-add-btn",
                onclick: move |_| show_input.set(true),
                i { class: "bi bi-plus-lg" }
            }
        }
    }
}

#[component]
fn OpenPdfButton() -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let render_ch = use_context::<crate::app::RenderChannel>();
    let mut error_msg = use_signal(|| None::<String>);

    rsx! {
        button {
            class: "sidebar-open-btn",
            onclick: move |_| {
                let file = rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_title("Open PDF")
                    .pick_file();

                if let Some(path) = file {
                    let path_str = path.to_string_lossy().to_string();
                    let render_tx = render_ch.sender();
                    // Show viewer immediately with loading state
                    pdf_state.with_mut(|s| {
                        s.pdf_path = Some(path_str.clone());
                        s.is_loading_pages = true;
                    });
                    lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                    spawn(async move {
                        match crate::state::commands::open_pdf(&render_tx, &mut pdf_state, &path_str).await {
                            Ok(()) => {
                                pdf_state.with_mut(|s| s.is_loading_pages = false);
                                error_msg.set(None);
                            }
                            Err(e) => {
                                pdf_state.with_mut(|s| s.is_loading_pages = false);
                                error_msg.set(Some(format!("Failed: {e}")));
                            }
                        }
                    });
                }
            },
            "Open PDF"
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { class: "sidebar-error", "{err}" }
        }
    }
}

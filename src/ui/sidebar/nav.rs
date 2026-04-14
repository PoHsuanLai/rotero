use dioxus::prelude::*;

use crate::state::app_state::{DragPaper, LibraryState, LibraryView, PdfTabManager};
use rotero_db::Database;

use super::collections::{CollectionTree, NewCollectionButton, NewCollectionRow};
use super::context_menus::{CollectionContextMenu, RecentContextMenu, SidebarTagContextMenu};
use super::open_pdf::OpenPdfButton;
use super::tags::TagSection;
use super::{CollapsibleSection, SidebarItem, TagContextMenu};

#[component]
pub fn Sidebar(collapsed: bool, on_toggle: EventHandler<()>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();
    let state = lib_state.read();
    let view = state.view.clone();
    let papers = &state.papers;

    let total = papers.len();
    let favorites_count = papers.iter().filter(|p| p.status.is_favorite).count();
    let unread_count = papers.iter().filter(|p| !p.status.is_read).count();
    let recent_count = total.min(20);

    let mut recent_papers: Vec<_> = papers
        .iter()
        .filter(|p| p.links.pdf_path.is_some())
        .collect();
    recent_papers.sort_by(|a, b| b.status.date_modified.cmp(&a.status.date_modified));
    let recent_opened: Vec<_> = recent_papers.into_iter().take(5).collect();

    let mut coll_ctx = use_signal(|| None::<(String, String, f64, f64)>);
    let mut tag_ctx = use_signal(|| None::<TagContextMenu>);
    let mut recent_ctx = use_signal(|| None::<(String, f64, f64)>);

    // None = not editing, Some(None) = top-level, Some(Some(id)) = subcollection
    let new_coll_editing: Signal<Option<Option<String>>> = use_context();

    let mut drag_coll: Signal<Option<String>> =
        use_context_provider(|| Signal::new(None::<String>));
    let mut drag_paper = use_context::<Signal<DragPaper>>();
    let _drop_hover: Signal<Option<String>> = use_context_provider(|| Signal::new(None::<String>));

    let sidebar_class = if collapsed {
        "sidebar sidebar--collapsed"
    } else {
        "sidebar"
    };

    if collapsed {
        return rsx! {
            div { class: "{sidebar_class}",
                button {
                    class: "sidebar-collapsed-btn",
                    title: "Expand sidebar",
                    onclick: move |_| on_toggle.call(()),
                    i { class: "bi bi-layout-sidebar-inset" }
                }
                button {
                    class: "sidebar-collapsed-btn",
                    title: "All Papers",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                    },
                    i { class: "bi bi-journal-text" }
                }
                button {
                    class: "sidebar-collapsed-btn",
                    title: "Recently Added",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::RecentlyAdded);
                    },
                    i { class: "bi bi-clock" }
                }
                button {
                    class: "sidebar-collapsed-btn",
                    title: "Favorites",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::Favorites);
                    },
                    i { class: "bi bi-star" }
                }
                button {
                    class: "sidebar-collapsed-btn",
                    title: "Unread",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::Unread);
                    },
                    i { class: "bi bi-circle" }
                }
                div { class: "sidebar-spacer" }
                crate::ui::settings::SettingsButton {}
            }
        };
    }

    rsx! {
        div { class: "{sidebar_class}",
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
                    title: "Collapse sidebar",
                    onclick: move |_| on_toggle.call(()),
                    i { class: "bi bi-layout-sidebar-inset" }
                }
            }

            OpenPdfButton {}

            CollapsibleSection { title: "Library", initially_open: true,
                SidebarItem {
                    label: "All Papers".to_string(),
                    count: Some(total),
                    icon: "doc",
                    active: view == LibraryView::AllPapers,
                    view: LibraryView::AllPapers,
                }
                SidebarItem {
                    label: "Recently Added".to_string(),
                    count: Some(recent_count),
                    icon: "clock",
                    active: view == LibraryView::RecentlyAdded,
                    view: LibraryView::RecentlyAdded,
                }
                SidebarItem {
                    label: "Favorites".to_string(),
                    count: Some(favorites_count),
                    icon: "star",
                    active: view == LibraryView::Favorites,
                    view: LibraryView::Favorites,
                }
                SidebarItem {
                    label: "Unread".to_string(),
                    count: Some(unread_count),
                    icon: "circle",
                    active: view == LibraryView::Unread,
                    view: LibraryView::Unread,
                }
                SidebarItem {
                    label: format!("Duplicates"),
                    count: None,
                    icon: "copy",
                    active: view == LibraryView::Duplicates,
                    view: LibraryView::Duplicates,
                }
            }

            if !recent_opened.is_empty() {
                CollapsibleSection { title: "Recent", initially_open: true,
                    for paper in recent_opened.iter() {
                        {
                            let paper_id = paper.id.clone().unwrap_or_default();
                            let title = paper.title.clone();
                            let pdf_rel = paper.links.pdf_path.clone();
                            let recent_icon = if paper.links.pdf_path.is_some() { "bi bi-file-earmark-pdf" } else { "bi bi-file-earmark-text" };
                            let db_recent = db.clone();
                            let truncated = if title.len() > 35 {
                                crate::ui::truncate_text(&title, 35)
                            } else {
                                title.clone()
                            };
                            let pid_drag = paper_id.clone();
                            let pid_open = paper_id.clone();
                            let pid_ctx = paper_id.clone();
                            rsx! {
                                div {
                                    class: "sidebar-collection-item",
                                    draggable: "true",
                                    ondragstart: move |_| {
                                        drag_paper.set(DragPaper(Some(vec![pid_drag.clone()])));
                                    },
                                    ondragend: move |_| {
                                        spawn(async move {
                                            drag_paper.set(DragPaper(None));
                                        });
                                    },
                                    onmouseup: move |evt: Event<MouseData>| {
                                        if drag_paper().0.is_none()
                                            && evt.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary)
                                            && let Some(ref rel_path) = pdf_rel
                                        {
                                            crate::state::commands::open_paper_pdf(&db_recent, &mut tabs, &mut lib_state, &config, &dpr_sig, &pid_open, rel_path, &title);
                                        }
                                    },
                                    oncontextmenu: move |evt: Event<MouseData>| {
                                        evt.prevent_default();
                                        recent_ctx.set(Some((pid_ctx.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                                    },
                                    i { class: "sidebar-collection-icon {recent_icon}" }
                                    span { class: "sidebar-collection-name", "{truncated}" }
                                }
                            }
                        }
                    }
                }
            }

            CollapsibleSection { title: "Collections", initially_open: true,
                action: rsx! { NewCollectionButton {} },
                CollectionTree { collections: state.collections.clone(), parent_id: None, depth: 0, ctx_menu: coll_ctx }
                if state.collections.is_empty() && new_coll_editing().is_none() {
                    p { class: "sidebar-empty", "No collections" }
                }
                if new_coll_editing() == Some(None) {
                    NewCollectionRow { parent_id: None, depth: 0 }
                }
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
                                        let did = dragged_id.clone();
                                        spawn(async move {
                                            if let Ok(()) = rotero_db::collections::reparent_collection(db.conn(), &did, None).await {
                                                let did2 = did.clone();
                                                lib_state.with_mut(|s| {
                                                    if let Some(c) = s.collections.iter_mut().find(|c| c.id.as_deref() == Some(did2.as_str())) {
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

            TagSection { tags: state.tags.clone(), ctx_menu: tag_ctx }

            if !state.saved_searches.is_empty() {
                CollapsibleSection { title: "Saved Searches", initially_open: true,
                    for search in state.saved_searches.iter() {
                        {
                            let search_id = search.id.clone().unwrap_or_default();
                            let search_name = search.name.clone();
                            let search_query = search.query.clone();
                            let is_active = view == LibraryView::SavedSearch(search_id.clone());
                            let item_class = if is_active { "sidebar-filter-item sidebar-filter-item--active" } else { "sidebar-filter-item" };
                            let db_del = db.clone();
                            let sid_click = search_id.clone();
                            let sid_del = search_id.clone();
                            rsx! {
                                div {
                                    key: "saved-search-{search_id}",
                                    class: "{item_class}",
                                    onclick: move |_| {
                                        let sid = sid_click.clone();
                                        lib_state.with_mut(|s| {
                                            s.search.query = search_query.clone();
                                            s.view = LibraryView::SavedSearch(sid);
                                        });
                                    },
                                    div { class: "sidebar-filter-left",
                                        i { class: "bi bi-search sidebar-filter-icon" }
                                        span { class: "sidebar-filter-label", "{search_name}" }
                                    }
                                    button {
                                        class: "btn--danger-sm",
                                        onclick: move |evt| {
                                            evt.stop_propagation();
                                            let db = db_del.clone();
                                            let sid = sid_del.clone();
                                            spawn(async move {
                                                let _ = rotero_db::saved_searches::delete_saved_search(db.conn(), &sid).await;
                                                if let Ok(searches) = rotero_db::saved_searches::list_saved_searches(db.conn()).await {
                                                    lib_state.with_mut(|s| s.saved_searches = searches);
                                                }
                                            });
                                        },
                                        "x"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "sidebar-spacer" }
            crate::ui::settings::SettingsButton {}

            if let Some((coll_id, coll_name, mx, my)) = coll_ctx() {
                CollectionContextMenu {
                    coll_id,
                    coll_name,
                    x: mx,
                    y: my,
                    on_close: move |_| coll_ctx.set(None),
                }
            }

            if let Some((tag_id, tag_name, _tag_color, mx, my)) = tag_ctx() {
                SidebarTagContextMenu {
                    tag_id,
                    tag_name,
                    x: mx,
                    y: my,
                    on_close: move |_| tag_ctx.set(None),
                }
            }

            if let Some((paper_id, mx, my)) = recent_ctx() {
                RecentContextMenu {
                    paper_id,
                    x: mx,
                    y: my,
                    on_close: move |_| recent_ctx.set(None),
                }
            }
        }
    }
}

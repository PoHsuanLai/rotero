use dioxus::prelude::*;

use crate::state::app_state::{DragPaper, LibraryState, LibraryView};
use rotero_db::Database;
use rotero_models::Collection;

#[component]
pub(crate) fn CollectionTree(
    collections: Vec<Collection>,
    parent_id: Option<String>,
    depth: u32,
    ctx_menu: Signal<Option<(String, String, f64, f64)>>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let new_coll_editing = use_context::<Signal<Option<Option<String>>>>();
    let mut drag_coll = use_context::<Signal<Option<String>>>();
    let mut drag_paper = use_context::<Signal<DragPaper>>();
    let mut drop_hover = use_context::<Signal<Option<String>>>();
    let mut coll_ctx = ctx_menu;
    let lib = lib_state.read();
    let view = lib.view.clone();

    let children: Vec<_> = collections
        .iter()
        .filter(|c| c.parent_id == parent_id)
        .cloned()
        .collect();

    if children.is_empty() && new_coll_editing() != Some(parent_id.clone()) {
        return rsx! {};
    }

    let indent = depth * 16;

    rsx! {
        for coll in children.iter() {
            {
                let coll_id = coll.id.clone().unwrap_or_default();
                let coll_name = coll.name.clone();
                let is_active = view == LibraryView::Collection(coll_id.clone());
                let class = if is_active {
                    "sidebar-collection-item sidebar-collection-item--active"
                } else {
                    "sidebar-collection-item"
                };

                let has_children = collections.iter().any(|c| c.parent_id.as_deref() == Some(coll_id.as_str()));
                let collections_clone = collections.clone();
                let creating_under_this = new_coll_editing() == Some(Some(coll_id.clone()));

                let folder_icon = if is_active {
                    "bi bi-folder2-open"
                } else if has_children {
                    "bi bi-folder-fill"
                } else {
                    "bi bi-folder"
                };

                let is_drag_active = (drag_coll().is_some() && drag_coll().as_deref() != Some(coll_id.as_str())) || drag_paper().0.is_some();
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

                let cid_click = coll_id.clone();
                let cid_ctx = coll_id.clone();
                let cid_drag = coll_id.clone();
                let cid_enter = coll_id.clone();
                let cid_leave = coll_id.clone();
                let cid_drop = coll_id.clone();
                let cid_child = coll_id.clone();
                let cid_newrow = coll_id.clone();

                rsx! {
                    div {
                        key: "coll-{coll_id}",
                        class: "{item_class}",
                        style: "padding-left: {indent + 8}px;",
                        draggable: "true",
                        onmouseup: move |evt: Event<MouseData>| {
                            if drag_coll().is_none()
                                && evt.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                                    lib_state.with_mut(|s| s.view = LibraryView::Collection(cid_click.clone()));
                                }
                        },
                        oncontextmenu: {
                            let name = coll_name.clone();
                            move |evt: Event<MouseData>| {
                                evt.prevent_default();
                                let coords = evt.page_coordinates();
                                coll_ctx.set(Some((cid_ctx.clone(), name.clone(), coords.x, coords.y)));
                            }
                        },
                        ondragstart: move |_| {
                            drag_coll.set(Some(cid_drag.clone()));
                        },
                        ondragenter: move |_| {
                            drop_hover.set(Some(format!("coll-{}", cid_enter)));
                        },
                        ondragleave: move |_| {
                            if drop_hover().as_deref() == Some(&format!("coll-{}", cid_leave)) {
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
                                if dragged_id != cid_drop {
                                    let db = db_for_drop.clone();
                                    let target = cid_drop.clone();
                                    spawn(async move {
                                        if let Ok(()) = rotero_db::collections::reparent_collection(db.conn(), &dragged_id, Some(&target)).await {
                                            let did = dragged_id.clone();
                                            let target2 = target.clone();
                                            lib_state.with_mut(|s| {
                                                if let Some(c) = s.collections.iter_mut().find(|c| c.id.as_deref() == Some(did.as_str())) {
                                                    c.parent_id = Some(target2);
                                                }
                                            });
                                        }
                                    });
                                }
                                drag_coll.set(None);
                            } else if let Some(paper_id) = drag_paper().0.clone() {
                                let db = db_for_paper_drop.clone();
                                let target = cid_drop.clone();
                                spawn(async move {
                                    if let Ok(()) = rotero_db::collections::add_paper_to_collection(db.conn(), &paper_id, &target).await {
                                        let current_view = lib_state.read().view.clone();
                                        if current_view == LibraryView::Collection(target.clone())
                                            && let Ok(ids) = rotero_db::collections::list_paper_ids_in_collection(db.conn(), &target).await {
                                                lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
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
                    if has_children || creating_under_this {
                        CollectionTree {
                            collections: collections_clone,
                            parent_id: Some(cid_child),
                            depth: depth + 1,
                            ctx_menu: coll_ctx,
                        }
                    }
                    if creating_under_this {
                        NewCollectionRow { parent_id: Some(cid_newrow), depth: depth + 1 }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn NewCollectionButton() -> Element {
    let mut editing = use_context::<Signal<Option<Option<String>>>>();
    let lib_state = use_context::<Signal<LibraryState>>();

    rsx! {
        button {
            class: "sidebar-add-btn",
            onclick: move |_| {
                let parent = match &lib_state.read().view {
                    LibraryView::Collection(id) => Some(id.clone()),
                    _ => None,
                };
                editing.set(Some(parent));
            },
            i { class: "bi bi-plus-lg" }
        }
    }
}

#[component]
pub(crate) fn NewCollectionRow(parent_id: Option<String>, depth: u32) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut editing = use_context::<Signal<Option<Option<String>>>>();
    let mut name_value = use_signal(String::new);
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
                onmounted: move |evt| { drop(evt.set_focus(true)); },
                onkeydown: move |evt| {
                    match evt.key() {
                        Key::Enter => {
                            let name = name_value().trim().to_string();
                            if !name.is_empty() {
                                submitted.set(true);
                                let mut coll = rotero_models::Collection::new(name);
                                coll.parent_id = parent_id.clone();
                                let db = db.clone();
                                spawn(async move {
                                    if let Ok(id) = rotero_db::collections::insert_collection(db.conn(), &coll).await {
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

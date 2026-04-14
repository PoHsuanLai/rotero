use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, LibraryView};
use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use rotero_db::Database;

#[component]
pub fn CollectionContextMenu(
    coll_id: String,
    coll_name: String,
    x: f64,
    y: f64,
    on_close: EventHandler<()>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut new_coll_editing: Signal<Option<Option<String>>> = use_context();
    let mut renaming = use_signal(|| false);
    let mut rename_value = use_signal(|| coll_name.clone());

    rsx! {
        if renaming() {
            ContextMenu {
                x,
                y,
                on_close: move |_| {
                    renaming.set(false);
                    on_close.call(());
                },
                div { class: "context-menu-rename",
                    input {
                        class: "input input--sm",
                        r#type: "text",
                        value: "{rename_value}",
                        oninput: move |evt| rename_value.set(evt.value()),
                        onkeypress: {
                            let cid = coll_id.clone();
                            let db = db.clone();
                            move |evt| {
                                if evt.key() == Key::Enter {
                                    let new_name = rename_value().trim().to_string();
                                    if !new_name.is_empty() {
                                        let db = db.clone();
                                        let cid = cid.clone();
                                        spawn(async move {
                                            if let Ok(()) = rotero_db::collections::rename_collection(db.conn(), &cid, &new_name).await {
                                                let cid2 = cid.clone();
                                                lib_state.with_mut(|s| {
                                                    if let Some(c) = s.collections.iter_mut().find(|c| c.id.as_deref() == Some(cid2.as_str())) {
                                                        c.name = new_name;
                                                    }
                                                });
                                            }
                                            renaming.set(false);
                                            on_close.call(());
                                        });
                                    }
                                }
                            }
                        },
                    }
                }
            }
        } else {
            ContextMenu {
                x,
                y,
                on_close: move |_| {
                    on_close.call(());
                },

                ContextMenuItem {
                    label: "New subcollection".to_string(),
                    icon: Some("bi-folder-plus".to_string()),
                    on_click: {
                        let cid = coll_id.clone();
                        move |_| {
                            new_coll_editing.set(Some(Some(cid.clone())));
                            on_close.call(());
                        }
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
                    on_click: {
                        let cid = coll_id.clone();
                        let db = db.clone();
                        move |_| {
                            let db = db.clone();
                            let cid = cid.clone();
                            spawn(async move {
                                if let Ok(()) = rotero_db::collections::delete_collection(db.conn(), &cid).await {
                                    let cid2 = cid.clone();
                                    lib_state.with_mut(|s| {
                                        s.collections.retain(|c| c.id.as_deref() != Some(cid2.as_str()));
                                        if s.view == LibraryView::Collection(cid.clone()) {
                                            s.view = LibraryView::AllPapers;
                                        }
                                    });
                                }
                            });
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub fn SidebarTagContextMenu(
    tag_id: String,
    tag_name: String,
    x: f64,
    y: f64,
    on_close: EventHandler<()>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut renaming = use_signal(|| false);
    let mut rename_value = use_signal(|| tag_name.clone());

    let colors = [
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
                x,
                y,
                on_close: move |_| {
                    renaming.set(false);
                    on_close.call(());
                },
                div { class: "context-menu-rename",
                    input {
                        class: "input input--sm",
                        r#type: "text",
                        value: "{rename_value}",
                        oninput: move |evt| rename_value.set(evt.value()),
                        onkeypress: {
                            let tid = tag_id.clone();
                            let db = db.clone();
                            move |evt| {
                                if evt.key() == Key::Enter {
                                    let new_name = rename_value().trim().to_string();
                                    if !new_name.is_empty() {
                                        let db = db.clone();
                                        let tid = tid.clone();
                                        spawn(async move {
                                            if let Ok(()) = rotero_db::tags::rename_tag(db.conn(), &tid, &new_name).await {
                                                let tid2 = tid.clone();
                                                lib_state.with_mut(|s| {
                                                    if let Some(t) = s.tags.iter_mut().find(|t| t.id.as_deref() == Some(tid2.as_str())) {
                                                        t.name = new_name;
                                                    }
                                                });
                                            }
                                            renaming.set(false);
                                            on_close.call(());
                                        });
                                    }
                                }
                            }
                        },
                    }
                }
            }
        } else {
            ContextMenu {
                x,
                y,
                on_close: move |_| {
                    on_close.call(());
                },

                ContextMenuItem {
                    label: "Filter by tag".to_string(),
                    icon: Some("bi-funnel".to_string()),
                    on_click: {
                        let tid = tag_id.clone();
                        move |_| {
                            lib_state.with_mut(|s| s.view = LibraryView::Tag(tid.clone()));
                            on_close.call(());
                        }
                    },
                }

                div { class: "context-menu-item",
                    i { class: "context-menu-icon bi bi-palette" }
                    span { class: "context-menu-label", "Color" }
                    div { class: "context-menu-colors",
                        for (color, _label) in colors.iter() {
                            {
                                let color = color.to_string();
                                let color_for_click = color.clone();
                                let db = db.clone();
                                rsx! {
                                    span {
                                        class: "context-menu-color-swatch",
                                        style: "background: {color};",
                                        onclick: {
                                            let tid = tag_id.clone();
                                            move |evt: Event<MouseData>| {
                                                evt.stop_propagation();
                                                let c = color_for_click.clone();
                                                let db = db.clone();
                                                let tid = tid.clone();
                                                spawn(async move {
                                                    if let Ok(()) = rotero_db::tags::update_tag_color(db.conn(), &tid, &c).await {
                                                        let tid2 = tid.clone();
                                                        lib_state.with_mut(|s| {
                                                            if let Some(t) = s.tags.iter_mut().find(|t| t.id.as_deref() == Some(tid2.as_str())) {
                                                                t.color = Some(c);
                                                            }
                                                        });
                                                    }
                                                    on_close.call(());
                                                });
                                            }
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
                    on_click: {
                        let tid = tag_id.clone();
                        let db = db.clone();
                        move |_| {
                            let db = db.clone();
                            let tid = tid.clone();
                            spawn(async move {
                                if let Ok(()) = rotero_db::tags::delete_tag(db.conn(), &tid).await {
                                    let tid2 = tid.clone();
                                    lib_state.with_mut(|s| {
                                        s.tags.retain(|t| t.id.as_deref() != Some(tid2.as_str()));
                                        if s.view == LibraryView::Tag(tid.clone()) {
                                            s.view = LibraryView::AllPapers;
                                        }
                                    });
                                }
                            });
                        }
                    },
                }
            }
        }
    }
}

#[component]
pub fn RecentContextMenu(paper_id: String, x: f64, y: f64, on_close: EventHandler<()>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();

    rsx! {
        ContextMenu {
            x,
            y,
            on_close: move |_| {
                on_close.call(());
            },

            ContextMenuItem {
                label: "Show in library".to_string(),
                icon: Some("bi-collection".to_string()),
                on_click: {
                    let pid = paper_id.clone();
                    move |_| {
                        lib_state.with_mut(|s| {
                            s.view = LibraryView::AllPapers;
                            s.select_one(pid.clone());
                        });
                        on_close.call(());
                    }
                },
            }
        }
    }
}

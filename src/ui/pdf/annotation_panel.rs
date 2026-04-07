use dioxus::prelude::*;

use super::super::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuSeparator};
use super::AnnCtxState;
use crate::state::app_state::{AnnotationContextInfo, PdfTabManager, TabId};
use rotero_db::Database;
use rotero_models::AnnotationType;

#[component]
pub(crate) fn AnnotationPanel(tab_id: TabId) -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let mut ann_ctx = use_context::<AnnCtxState>();
    let annotations = tabs.read().tab().annotations.clone();

    rsx! {
        div { class: "annotation-panel",
            div { class: "annotation-panel-header",
                span { "Annotations ({annotations.len()})" }
                if !annotations.is_empty() {
                    {
                        let db_extract = db.clone();
                        let anns_for_extract = annotations.clone();
                        rsx! {
                            button {
                                class: "btn btn--ghost btn--sm",
                                title: "Extract annotations to a note",
                                onclick: move |_| {
                                    let db = db_extract.clone();
                                    let anns = anns_for_extract.clone();
                                    let paper_id = tabs.read().tab().paper_id.clone();
                                    let paper_title = tabs.read().tab().title.clone();
                                    spawn(async move {
                                        if let Some(pid) = paper_id {
                                            let mut body = String::new();
                                            let mut current_page: Option<i32> = None;
                                            for ann in &anns {
                                                if current_page != Some(ann.page) {
                                                    if !body.is_empty() {
                                                        body.push('\n');
                                                    }
                                                    body.push_str(&format!("## Page {}\n", ann.page + 1));
                                                    current_page = Some(ann.page);
                                                }
                                                let type_label = match ann.ann_type {
                                                    rotero_models::AnnotationType::Highlight => "Highlight",
                                                    rotero_models::AnnotationType::Note => "Note",
                                                    rotero_models::AnnotationType::Area => "Area",
                                                    rotero_models::AnnotationType::Underline => "Underline",
                                                    rotero_models::AnnotationType::Ink => "Ink",
                                                    rotero_models::AnnotationType::Text => "Text",
                                                };
                                                let content = ann.content.as_deref().unwrap_or("");
                                                if content.is_empty() {
                                                    body.push_str(&format!("- [{type_label}] ({}) \n", ann.color));
                                                } else {
                                                    body.push_str(&format!("- [{type_label}] \"{content}\" ({}) \n", ann.color));
                                                }
                                            }
                                            let title = format!("Annotations from {}", paper_title);
                                            let mut note = rotero_models::Note::new(pid, title);
                                            note.body = body;
                                            let _ = rotero_db::notes::insert_note(db.conn(), &note).await;
                                        }
                                    });
                                },
                                "Extract to Note"
                            }
                        }
                    }
                }
            }
            if annotations.is_empty() {
                div { class: "annotation-panel-empty", "No annotations yet. Use the Highlight or Note tool to add annotations." }
            } else {
                div { class: "annotation-panel-list",
                    for ann in annotations.iter() {
                        {
                            let ann_id = ann.id.clone().unwrap_or_default();
                            let page = ann.page;
                            let color = ann.color.clone();
                            let ann_type = ann.ann_type;
                            let content = ann.content.clone().unwrap_or_default();
                            let mut editing = use_signal(|| false);
                            let mut edit_value = use_signal(|| content.clone());
                            let db_for_delete = db.clone();
                            let db_for_save = db.clone();
                            let type_label = match ann_type {
                                AnnotationType::Highlight => "Highlight",
                                AnnotationType::Note => "Note",
                                AnnotationType::Area => "Area",
                                AnnotationType::Underline => "Underline",
                                AnnotationType::Ink => "Ink",
                                AnnotationType::Text => "Text",
                            };
                            let ctx_color = color.clone();
                            let ctx_content = content.clone();
                            let aid_ctx = ann_id.clone();
                            let aid_del = ann_id.clone();
                            let aid_save = ann_id.clone();
                            rsx! {
                                div {
                                    key: "panel-ann-{ann_id}",
                                    class: "annotation-item",
                                    style: "border-left-color: {color};",
                                    oncontextmenu: move |evt: Event<MouseData>| {
                                        evt.prevent_default();
                                        ann_ctx.set(Some(AnnotationContextInfo {
                                            annotation_id: aid_ctx.clone(),
                                            ann_type,
                                            page,
                                            color: ctx_color.clone(),
                                            content: ctx_content.clone(),
                                            x: evt.client_coordinates().x,
                                            y: evt.client_coordinates().y,
                                        }));
                                    },
                                    div { class: "annotation-item-header",
                                        div { class: "annotation-item-meta",
                                            div { class: "annotation-color-dot", style: "background: {color};" }
                                            span { class: "annotation-type-label", "{type_label}" }
                                            span { class: "annotation-page-label", "p.{page + 1}" }
                                        }
                                        button {
                                            class: "btn--danger-sm",
                                            onclick: move |_| {
                                                let db = db_for_delete.clone();
                                                let aid = aid_del.clone();
                                                let deleted_ann = tabs.read().tab().annotations.iter().find(|a| a.id.as_deref() == Some(aid.as_str())).cloned();
                                                let aid2 = aid.clone();
                                                spawn(async move {
                                                    if let Ok(()) = rotero_db::annotations::delete_annotation(db.conn(), &aid).await {
                                                        if let Some(ann) = deleted_ann {
                                                            undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Delete(ann)));
                                                        }
                                                        let aid3 = aid2.clone();
                                                        tabs.with_mut(|m| m.tab_mut().annotations.retain(|a| a.id.as_deref() != Some(aid3.as_str())));
                                                    }
                                                });
                                            },
                                            "x"
                                        }
                                    }
                                    if ann_type == AnnotationType::Note {
                                        if editing() {
                                            div { class: "annotation-edit-area",
                                                textarea { class: "textarea", value: "{edit_value}", oninput: move |evt| edit_value.set(evt.value()) }
                                                div { class: "annotation-edit-actions",
                                                    button {
                                                        class: "btn--save-sm",
                                                        onclick: move |_| {
                                                            let new_content = edit_value();
                                                            let old_content = content.clone();
                                                            let db = db_for_save.clone();
                                                            let nc = new_content.clone();
                                                            let aid = aid_save.clone();
                                                            spawn(async move {
                                                                let opt = if nc.is_empty() { None } else { Some(nc.as_str()) };
                                                                if let Ok(()) = rotero_db::annotations::update_annotation_content(db.conn(), &aid, opt).await {
                                                                    let old = if old_content.is_empty() { None } else { Some(old_content) };
                                                                    let new = if new_content.is_empty() { None } else { Some(new_content.clone()) };
                                                                    let aid2 = aid.clone();
                                                                    undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::UpdateContent { id: aid2, old, new }));
                                                                    let aid3 = aid.clone();
                                                                    tabs.with_mut(|m| {
                                                                        if let Some(a) = m.tab_mut().annotations.iter_mut().find(|a| a.id.as_deref() == Some(aid3.as_str())) {
                                                                            a.content = if new_content.is_empty() { None } else { Some(new_content.clone()) };
                                                                        }
                                                                    });
                                                                }
                                                                editing.set(false);
                                                            });
                                                        },
                                                        "Save"
                                                    }
                                                    button { class: "btn--cancel-sm", onclick: move |_| editing.set(false), "Cancel" }
                                                }
                                            }
                                        } else {
                                            div {
                                                class: "annotation-note-content",
                                                onclick: move |_| { edit_value.set(content.clone()); editing.set(true); },
                                                if content.is_empty() {
                                                    span { class: "annotation-note-empty", "Click to add note..." }
                                                } else { "{content}" }
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
}

#[component]
pub(crate) fn AnnotationContextMenu() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let mut undo_stack = use_context::<Signal<crate::state::undo::UndoStack>>();
    let mut ann_ctx = use_context::<AnnCtxState>();

    let Some(ctx) = ann_ctx() else {
        return rsx! {};
    };

    let ctx_ann_id = ctx.annotation_id;
    let ctx_type = ctx.ann_type;
    let ctx_page = ctx.page;
    let ctx_old_color = ctx.color;
    let ctx_content = ctx.content;
    let mx = ctx.x;
    let my = ctx.y;
    let ctx_ann_id_del = ctx_ann_id.clone();
    let db_color = db.clone();
    let db_delete = db.clone();
    let colors = [
        ("#ffff00", "Yellow"),
        ("#ff6b6b", "Red"),
        ("#51cf66", "Green"),
        ("#339af0", "Blue"),
        ("#cc5de8", "Purple"),
        ("#ff922b", "Orange"),
    ];

    rsx! {
        ContextMenu {
            x: mx,
            y: my,
            on_close: move |_| {
                ann_ctx.set(None);
            },

            ContextMenuItem {
                label: format!("Go to page {}", ctx_page + 1),
                icon: Some("bi-arrow-right-circle".to_string()),
                on_click: move |_| {
                    let js = format!(
                        "let el = document.getElementById('pdf-page-{}'); if (el) el.scrollIntoView({{behavior: 'smooth'}})",
                        ctx_page
                    );
                    let _ = document::eval(&js);
                    ann_ctx.set(None);
                },
            }

            if ctx_type == AnnotationType::Note {
                ContextMenuItem {
                    label: "Edit note".to_string(),
                    icon: Some("bi-pencil".to_string()),
                    on_click: move |_| {
                        ann_ctx.set(None);
                    },
                }
            }

            if ctx_type == AnnotationType::Highlight && !ctx_content.is_empty() {
                {
                    let text = ctx_content.clone();
                    rsx! {
                        ContextMenuItem {
                            label: "Copy text".to_string(),
                            icon: Some("bi-clipboard".to_string()),
                            on_click: move |_| {
                                if let Ok(mut clip) = arboard::Clipboard::new() {
                                    let _ = clip.set_text(&*text);
                                }
                                ann_ctx.set(None);
                            },
                        }
                    }
                }
            }

            div { class: "context-menu-item",
                i { class: "context-menu-icon bi bi-palette" }
                span { class: "context-menu-label", "Color" }
                div { class: "context-menu-colors",
                    for (color, _label) in colors.iter() {
                        {
                            let color = color.to_string();
                            let color_for_click = color.clone();
                            let db_swatch = db_color.clone();
                            let old_color_for_swatch = ctx_old_color.clone();
                            let aid_swatch = ctx_ann_id.clone();
                            rsx! {
                                span {
                                    class: "context-menu-color-swatch",
                                    style: "background: {color};",
                                    onclick: move |evt| {
                                        evt.stop_propagation();
                                        let c = color_for_click.clone();
                                        let old_c = old_color_for_swatch.clone();
                                        let db = db_swatch.clone();
                                        let aid = aid_swatch.clone();
                                        spawn(async move {
                                            if let Ok(()) = rotero_db::annotations::update_annotation_color(db.conn(), &aid, &c).await {
                                                let aid2 = aid.clone();
                                                undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::UpdateColor { id: aid2, old: old_c, new: c.clone() }));
                                                let aid3 = aid.clone();
                                                tabs.with_mut(|m| {
                                                    if let Some(a) = m.tab_mut().annotations.iter_mut().find(|a| a.id.as_deref() == Some(aid3.as_str())) {
                                                        a.color = c;
                                                    }
                                                });
                                            }
                                            ann_ctx.set(None);
                                        });
                                    },
                                }
                            }
                        }
                    }
                }
            }

            ContextMenuSeparator {}

            ContextMenuItem {
                label: "Delete".to_string(),
                icon: Some("bi-trash".to_string()),
                danger: Some(true),
                on_click: {
                    let aid = ctx_ann_id_del.clone();
                    move |_| {
                        let db = db_delete.clone();
                        let aid = aid.clone();
                        let deleted_ann = tabs.read().tab().annotations.iter().find(|a| a.id.as_deref() == Some(aid.as_str())).cloned();
                        let aid2 = aid.clone();
                        spawn(async move {
                            if let Ok(()) = rotero_db::annotations::delete_annotation(db.conn(), &aid).await {
                                if let Some(ann) = deleted_ann {
                                    undo_stack.with_mut(|s| s.push(crate::state::undo::UndoAction::Delete(ann)));
                                }
                                let aid3 = aid2.clone();
                                tabs.with_mut(|m| m.tab_mut().annotations.retain(|a| a.id.as_deref() != Some(aid3.as_str())));
                            }
                        });
                        ann_ctx.set(None);
                    }
                },
            }
        }
    }
}

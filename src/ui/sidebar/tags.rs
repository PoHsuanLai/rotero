use dioxus::prelude::*;

use crate::state::app_state::{DragPaper, LibraryState, LibraryView};
use rotero_db::Database;

use super::TagContextMenu;

/// Own component so signal reads are tracked correctly
/// (CollapsibleSection children don't re-render when context signals change).
#[component]
pub(crate) fn TagSection(
    tags: Vec<rotero_models::Tag>,
    ctx_menu: Signal<Option<TagContextMenu>>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut drag_paper = use_context::<Signal<DragPaper>>();
    let mut drop_hover = use_context::<Signal<Option<String>>>();
    let mut tag_ctx = ctx_menu;
    let mut open = use_signal(|| true);

    let arrow_class = if open() {
        "bi bi-chevron-down"
    } else {
        "bi bi-chevron-right"
    };

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
                                    let tag_id = tag.id.clone().unwrap_or_default();
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
                                    let tid_click = tag_id.clone();
                                    let tid_ctx = tag_id.clone();
                                    let tid_enter = tag_id.clone();
                                    let tid_leave = tag_id.clone();
                                    let tid_drop = tag_id.clone();
                                    rsx! {
                                        span {
                                            class: "{tag_class}",
                                            style: "background: {bg};",
                                            onclick: move |_| {
                                                lib_state.with_mut(|s| s.view = LibraryView::Tag(tid_click.clone()));
                                            },
                                            oncontextmenu: move |evt: Event<MouseData>| {
                                                evt.prevent_default();
                                                tag_ctx.set(Some((tid_ctx.clone(), tag_name.clone(), tag_color.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                                            },
                                            ondragenter: move |_| {
                                                drop_hover.set(Some(format!("tag-{}", tid_enter)));
                                            },
                                            ondragleave: move |_| {
                                                if drop_hover().as_deref() == Some(&format!("tag-{}", tid_leave)) {
                                                    drop_hover.set(None);
                                                }
                                            },
                                            ondragover: move |evt| {
                                                evt.prevent_default();
                                            },
                                            ondrop: move |evt| {
                                                evt.prevent_default();
                                                drop_hover.set(None);
                                                if let Some(ref paper_id) = drag_paper().0 {
                                                    let db = db_for_tag_drop.clone();
                                                    let pid = paper_id.clone();
                                                    let tid = tid_drop.clone();
                                                    spawn(async move {
                                                        let _ = rotero_db::tags::add_tag_to_paper(db.conn(), &pid, &tid).await;
                                                        let current_view = lib_state.read().view.clone();
                                                        if current_view == LibraryView::Tag(tid.clone())
                                                            && let Ok(ids) = rotero_db::tags::list_paper_ids_by_tag(db.conn(), &tid).await {
                                                                lib_state.with_mut(|s| s.filter.tag_paper_ids = Some(ids));
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

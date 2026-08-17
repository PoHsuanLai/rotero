use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, MembershipRefresh};
use rotero_db::Database;
use rotero_models::collection_tree;

/// The collections a paper can belong to, rendered as the browser extension's
/// nested list of rows: subcollections sit indented under their parent. Rows the
/// paper is in are highlighted; clicking a row toggles membership live.
#[component]
pub fn CollectionsField(paper_id: String) -> Element {
    let lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let refresh = use_context::<Signal<MembershipRefresh>>();
    // The set of collection ids this paper currently belongs to.
    let mut member_ids = use_signal(Vec::<String>::new);

    // Reload whenever the selected paper changes or membership is bumped
    // elsewhere (context menu, drag-and-drop). Reading `refresh` subscribes this
    // effect to those bumps.
    {
        let db = db.clone();
        let pid = paper_id.clone();
        use_effect(move || {
            let _ = refresh.read().0;
            let db = db.clone();
            let pid = pid.clone();
            spawn(async move {
                if let Ok(colls) = db.list_collections_for_paper(&pid).await {
                    member_ids.set(colls.into_iter().filter_map(|c| c.id).collect());
                }
            });
        });
    }

    let collections = lib_state.read().collections.clone();
    let tree = collection_tree(&collections);
    let members = member_ids.read();

    rsx! {
        div { class: "coll-row-list",
            if tree.is_empty() {
                div { class: "detail-empty", "No collections yet" }
            }
            for (coll, depth, has_children) in tree.iter() {
                {
                    let cid = coll.id.clone().unwrap_or_default();
                    let cname = coll.name.clone();
                    let is_member = members.iter().any(|m| m == &cid);
                    let has_children = *has_children;
                    // Match the sidebar: parents show a filled folder, members open.
                    let icon = if is_member {
                        "bi bi-folder2-open"
                    } else if has_children {
                        "bi bi-folder-fill"
                    } else {
                        "bi bi-folder"
                    };
                    let row_class = if is_member {
                        "coll-row coll-row--detail coll-row--active"
                    } else {
                        "coll-row coll-row--detail"
                    };
                    // 12px base + 16px per level, mirroring the sidebar tree.
                    let indent = 12 + depth * 16;
                    let db = db.clone();
                    let pid = paper_id.clone();
                    let mut refresh = refresh;
                    rsx! {
                        div {
                            key: "coll-{cid}",
                            class: "{row_class}",
                            style: "padding-left: {indent}px;",
                            onclick: move |_| {
                                let cid = cid.clone();
                                // Flip local membership immediately so the row
                                // toggles on this click; DB write + refresh follow.
                                let now_member = !member_ids.read().contains(&cid);
                                member_ids.with_mut(|m| {
                                    if now_member {
                                        m.push(cid.clone());
                                    } else {
                                        m.retain(|x| x != &cid);
                                    }
                                });
                                let db = db.clone();
                                let pid = pid.clone();
                                spawn(async move {
                                    if now_member {
                                        if let Err(e) = db.add_paper_to_collection(&pid, &cid).await {
                                            let mut lib_state = lib_state;
                                            lib_state.with_mut(|s| s.report_error(format!("Could not add the paper to that collection: {e}")));
                                        }
                                    } else {
                                        if let Err(e) = db.remove_paper_from_collection(&pid, &cid).await {
                                            let mut lib_state = lib_state;
                                            lib_state.with_mut(|s| s.report_error(format!("Could not remove the paper from that collection: {e}")));
                                        }
                                    }
                                    refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                });
                            },
                            i { class: "coll-row-icon {icon}" }
                            span { class: "coll-row-name", "{cname}" }
                        }
                    }
                }
            }
        }
    }
}

/// The tags a paper can carry, rendered as the extension's toggle chips. Every
/// tag is shown as an outlined chip; the paper's tags are filled with the tag
/// color. Clicking a chip toggles membership live. A trailing input creates and
/// applies a brand-new tag.
#[component]
pub fn TagsField(paper_id: String) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let refresh = use_context::<Signal<MembershipRefresh>>();
    let mut member_ids = use_signal(Vec::<String>::new);
    let mut new_tag = use_signal(String::new);

    {
        let db = db.clone();
        let pid = paper_id.clone();
        use_effect(move || {
            let _ = refresh.read().0;
            let db = db.clone();
            let pid = pid.clone();
            spawn(async move {
                if let Ok(tags) = db.list_tags_for_paper(&pid).await {
                    member_ids.set(tags.into_iter().filter_map(|t| t.id).collect());
                }
            });
        });
    }

    let all_tags = lib_state.read().tags.clone();
    let members = member_ids.read();

    rsx! {
        div { class: "chip-wrap",
            for tag in all_tags.iter() {
                {
                    let tid = tag.id.clone().unwrap_or_default();
                    let tname = tag.name.clone();
                    let color = tag.color.clone().unwrap_or_else(|| "#6b7085".to_string());
                    let is_member = members.iter().any(|m| m == &tid);
                    // Every tag is a filled color pill (like the sidebar). Tags the
                    // paper doesn't have are dimmed via `--faded`, so membership
                    // still reads without any outline/dot variant.
                    let chip_class = if is_member { "chip" } else { "chip chip--faded" };
                    let db = db.clone();
                    let pid = paper_id.clone();
                    let mut refresh = refresh;
                    rsx! {
                        button {
                            key: "tag-{tid}",
                            class: "{chip_class}",
                            style: "background: {color}; border-color: {color};",
                            onclick: move |_| {
                                let tid = tid.clone();
                                // Flip the local membership immediately so the chip
                                // toggles on this click; the DB write + refresh
                                // follow. Without this the row wouldn't reflect the
                                // change until the async reload lands.
                                let now_member = !member_ids.read().contains(&tid);
                                member_ids.with_mut(|m| {
                                    if now_member {
                                        m.push(tid.clone());
                                    } else {
                                        m.retain(|x| x != &tid);
                                    }
                                });
                                let db = db.clone();
                                let pid = pid.clone();
                                spawn(async move {
                                    if now_member {
                                        if let Err(e) = db.add_tag_to_paper(&pid, &tid).await {
                                            let mut lib_state = lib_state;
                                            lib_state.with_mut(|s| s.report_error(format!("Could not attach that tag: {e}")));
                                        }
                                    } else {
                                        if let Err(e) = db.remove_tag_from_paper(&pid, &tid).await {
                                            let mut lib_state = lib_state;
                                            lib_state.with_mut(|s| s.report_error(format!("Could not remove that tag: {e}")));
                                        }
                                    }
                                    refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                });
                            },
                            "{tname}"
                        }
                    }
                }
            }

            input {
                id: "tag-editor-input",
                class: "chip-new",
                r#type: "text",
                placeholder: "+ New tag",
                value: "{new_tag}",
                onfocusin: crate::ui::keybindings::editable_focus_in,
                onfocusout: crate::ui::keybindings::editable_focus_out,
                oninput: move |evt| new_tag.set(evt.value()),
                onkeypress: {
                    let paper_id = paper_id.clone();
                    let db = db.clone();
                    let mut refresh = refresh;
                    move |evt| {
                        if evt.key() == Key::Enter {
                            let tag_name = new_tag().trim().to_string();
                            if tag_name.is_empty() { return; }
                            let db = db.clone();
                            let pid = paper_id.clone();
                            spawn(async move {
                                if let Ok(tag_id) = db.get_or_create_tag(&tag_name, None).await {
                                    if let Err(e) = db.add_tag_to_paper(&pid, &tag_id).await {
                                        let mut lib_state = lib_state;
                                        lib_state.with_mut(|s| s.report_error(format!("Could not attach that tag: {e}")));
                                    }
                                    if let Ok(tags) = db.list_tags().await {
                                        lib_state.with_mut(|s| s.tags = tags);
                                    }
                                    refresh.with_mut(|r| r.0 = r.0.wrapping_add(1));
                                }
                                new_tag.set(String::new());
                            });
                        }
                    }
                },
            }
        }
    }
}

use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use crate::ui::helpers::{item_type_icon, item_type_is_special, item_type_label};
use rotero_db::Database;

/// The "Title" label paired with the item-type badge, shared by the read-only
/// and editable title fields so both render an identical header row.
#[component]
pub fn TitleLabelRow(item_type: String) -> Element {
    let type_label = item_type_label(&item_type);
    let type_icon = item_type_icon(&item_type);
    let badge_class = if item_type_is_special(&item_type) {
        "type-badge type-badge--special"
    } else {
        "type-badge"
    };

    rsx! {
        div { class: "detail-title-row",
            label { class: "detail-label", "Title" }
            span { class: "{badge_class}", title: "{type_label}",
                i { class: "bi {type_icon}" }
                "{type_label}"
            }
        }
    }
}

/// The paper title as a static block. Used for web search results, which have
/// no stored record to rename.
#[component]
pub fn TitleField(title: String, item_type: String) -> Element {
    rsx! {
        div { class: "detail-field",
            TitleLabelRow { item_type }
            div { class: "detail-value detail-value--title", "{title}" }
        }
    }
}

/// The paper title as a click-to-edit field, mirroring the citation-key editor:
/// Enter or blur commits, Escape reverts. Committing writes the title alone and
/// patches the in-memory paper, so the card list and PDF tab retitle without a
/// reload.
///
/// `external_trigger` opens the editor without a click, letting the library
/// context menu's Rename action drive this editor from another component tree.
#[component]
pub fn EditableTitleField(
    paper_id: String,
    title: String,
    item_type: String,
    external_trigger: bool,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

    let mut editing = use_signal(|| false);
    let mut draft = use_signal(|| title.clone());
    // The component is reused across selections rather than remounted, so the
    // signals above keep their values when a different paper is selected. Track
    // which paper they belong to and reset when that changes, otherwise the
    // editor would stay open over the new paper holding the old draft.
    let mut editing_for = use_signal(|| paper_id.clone());

    {
        let paper_id = paper_id.clone();
        let title = title.clone();
        use_effect(move || {
            if *editing_for.peek() != paper_id {
                editing_for.set(paper_id.clone());
                editing.set(false);
                draft.set(title.clone());
            }
        });
    }

    // Honour a Rename request raised elsewhere, then clear it so re-selecting
    // this paper later doesn't reopen the editor.
    {
        let title = title.clone();
        use_effect(move || {
            if external_trigger && !*editing.peek() {
                draft.set(title.clone());
                editing.set(true);
                lib_state.with_mut(|s| s.rename_paper_id = None);
            }
        });
    }

    // Commit the draft: blank titles are rejected, and an unchanged title skips
    // the write entirely so a stray blur doesn't bump date_modified.
    let commit = {
        let db = db.clone();
        let paper_id = paper_id.clone();
        let original = title.clone();
        move || {
            let new_title = draft().trim().to_string();
            editing.set(false);
            if new_title.is_empty() || new_title == original {
                return;
            }
            let db = db.clone();
            let pid = paper_id.clone();
            spawn(async move {
                if let Err(e) = db.update_paper_title(&pid, &new_title).await {
                    lib_state
                        .with_mut(|s| s.report_error(format!("Could not rename the paper: {e}")));
                    return;
                }
                lib_state.with_mut(|s| {
                    if let Some(p) = s
                        .papers
                        .iter_mut()
                        .find(|p| p.id.as_deref() == Some(pid.as_str()))
                    {
                        p.title = new_title;
                    }
                });
            });
        }
    };

    rsx! {
        div { class: "detail-field",
            TitleLabelRow { item_type }
            if editing() {
                textarea {
                    class: "input detail-title-input",
                    rows: 3,
                    value: "{draft}",
                    autofocus: true,
                    onfocusin: crate::ui::keybindings::editable_focus_in,
                    oninput: move |evt| draft.set(evt.value()),
                    onkeydown: {
                        let mut commit = commit.clone();
                        let original = title.clone();
                        move |evt: Event<KeyboardData>| {
                            // Enter commits; the title is one line even though a
                            // textarea is used to wrap long ones.
                            if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                evt.prevent_default();
                                commit();
                            } else if evt.key() == Key::Escape {
                                draft.set(original.clone());
                                editing.set(false);
                            }
                        }
                    },
                    onfocusout: {
                        let mut commit = commit.clone();
                        move |evt| {
                            crate::ui::keybindings::editable_focus_out(evt);
                            commit();
                        }
                    },
                }
            } else {
                div {
                    class: "detail-value detail-value--title detail-value--editable",
                    title: "Click to rename",
                    onclick: {
                        let original = title.clone();
                        move |_| {
                            draft.set(original.clone());
                            editing.set(true);
                        }
                    },
                    "{title}"
                }
            }
        }
    }
}

use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use crate::ui::chat_panel::switch_to;
use rotero_db::Database;
use rotero_db::chat_sessions::ChatSubject;
use rotero_models::Paper;

use super::DetailShell;

#[component]
pub fn MultiSelectSummary() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut chat_state = use_context::<Signal<crate::agent::types::ChatState>>();
    let agent_channel = use_context::<crate::ui::chat_panel::AgentChannel>();

    let state = lib_state.read();
    let count = state.selection_count();
    let selected_papers: Vec<Paper> = state.selected_papers().into_iter().cloned().collect();
    let ids: Vec<String> = state.selected_paper_ids.iter().cloned().collect();
    drop(state);

    rsx! {
        DetailShell {
            div { class: "detail-header",
                h3 { class: "detail-heading", "{count} papers selected" }
                button {
                    class: "detail-close",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.clear_selection());
                    },
                    "\u{00d7}"
                }
            }

            div { class: "multi-select-actions",
                {
                    let ids_fav = ids.clone();
                    let db_fav = db.clone();
                    rsx! {
                        button {
                            class: "btn btn--ghost multi-select-btn",
                            onclick: move |_| {
                                let db = db_fav.clone();
                                let ids = ids_fav.clone();
                                spawn(async move {
                                    for pid in &ids {
                                        let _ = db.set_favorite(pid, true).await;
                                    }
                                    lib_state.with_mut(|s| {
                                        for pid in &ids {
                                            if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid.as_str())) {
                                                p.status.is_favorite = true;
                                            }
                                        }
                                    });
                                });
                            },
                            i { class: "bi bi-star" }
                            " Favorite All"
                        }
                    }
                }

                {
                    let ids_read = ids.clone();
                    let db_read = db.clone();
                    rsx! {
                        button {
                            class: "btn btn--ghost multi-select-btn",
                            onclick: move |_| {
                                let db = db_read.clone();
                                let ids = ids_read.clone();
                                spawn(async move {
                                    for pid in &ids {
                                        let _ = db.set_read(pid, true).await;
                                    }
                                    lib_state.with_mut(|s| {
                                        for pid in &ids {
                                            if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid.as_str())) {
                                                p.status.is_read = true;
                                            }
                                        }
                                    });
                                });
                            },
                            i { class: "bi bi-book-fill" }
                            " Mark All Read"
                        }
                    }
                }

                {
                    // The selection is the subject: several papers discussed
                    // together are one conversation, not one per paper.
                    let ids_chat = ids.clone();
                    let db_chat = db.clone();
                    rsx! {
                        button {
                            class: "btn btn--ghost multi-select-btn",
                            onclick: move |_| {
                                let subject = ChatSubject::Group(ids_chat.clone());
                                chat_state.with_mut(|s| s.panel_open = true);
                                switch_to(&mut chat_state, &agent_channel, &db_chat, subject);
                            },
                            i { class: "bi bi-chat-dots" }
                            " Chat About These"
                        }
                    }
                }

                {
                    let ids_del = ids.clone();
                    rsx! {
                        button {
                            class: "btn btn--danger multi-select-btn",
                            onclick: move |_| {
                                lib_state.with_mut(|s| {
                                    s.confirm_delete = Some(ids_del.clone());
                                });
                            },
                            i { class: "bi bi-trash" }
                            " Delete All"
                        }
                    }
                }
            }

            // List of selected papers
            div { class: "multi-select-list",
                for paper in selected_papers.iter() {
                    {
                        let pid = paper.id.clone().unwrap_or_default();
                        let title = paper.title.clone();
                        let authors = paper.formatted_authors();
                        let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                        let journal = paper.publication.journal.clone().unwrap_or_default();
                        rsx! {
                            div {
                                key: "{pid}",
                                class: "multi-select-card",
                                div { class: "multi-select-card-body",
                                    div { class: "multi-select-card-title", "{title}" }
                                    div { class: "multi-select-card-meta",
                                        "{authors}"
                                        if !year.is_empty() {
                                            " \u{00b7} {year}"
                                        }
                                        if !journal.is_empty() {
                                            " \u{00b7} {journal}"
                                        }
                                    }
                                }
                                button {
                                    class: "multi-select-card-remove",
                                    title: "Deselect",
                                    onclick: move |_| {
                                        lib_state.with_mut(|s| {
                                            s.selected_paper_ids.remove(&pid);
                                        });
                                    },
                                    "\u{00d7}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

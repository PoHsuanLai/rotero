use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;
use rotero_models::Paper;

#[component]
pub fn MultiSelectSummary() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

    let state = lib_state.read();
    let count = state.selection_count();
    let selected_papers: Vec<Paper> = state.selected_papers().into_iter().cloned().collect();
    let ids: Vec<String> = state.selected_paper_ids.iter().cloned().collect();
    drop(state);

    rsx! {
        div { class: "paper-detail",
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
                                        let _ = rotero_db::papers::set_favorite(db.conn(), pid, true).await;
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
                                        let _ = rotero_db::papers::set_read(db.conn(), pid, true).await;
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

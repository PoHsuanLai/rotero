use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::LibraryState;

#[component]
pub fn PaperDetail() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();

    let state = lib_state.read();
    let paper = match state.selected_paper() {
        Some(p) => p.clone(),
        None => return rsx! {},
    };
    drop(state);

    let paper_id = paper.id.unwrap_or(0);
    let authors_display = if paper.authors.is_empty() {
        "Unknown".to_string()
    } else {
        paper.authors.join(", ")
    };

    rsx! {
        div { class: "paper-detail",
            style: "width: 350px; border-left: 1px solid #eee; padding: 16px; overflow-y: auto; font-size: 14px;",

            // Close button
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px;",
                h3 { style: "margin: 0; font-size: 16px;", "Details" }
                button {
                    style: "padding: 2px 8px; border: 1px solid #ddd; background: #fff; border-radius: 4px; cursor: pointer;",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.selected_paper_id = None);
                    },
                    "×"
                }
            }

            // Title
            div { style: "margin-bottom: 12px;",
                label { style: "font-weight: 600; font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.5px;", "Title" }
                div { style: "margin-top: 4px;", "{paper.title}" }
            }

            // Authors
            div { style: "margin-bottom: 12px;",
                label { style: "font-weight: 600; font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.5px;", "Authors" }
                div { style: "margin-top: 4px;", "{authors_display}" }
            }

            // Year
            if let Some(year) = paper.year {
                div { style: "margin-bottom: 12px;",
                    label { style: "font-weight: 600; font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.5px;", "Year" }
                    div { style: "margin-top: 4px;", "{year}" }
                }
            }

            // Journal
            if let Some(ref journal) = paper.journal {
                div { style: "margin-bottom: 12px;",
                    label { style: "font-weight: 600; font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.5px;", "Journal" }
                    div { style: "margin-top: 4px;", "{journal}" }
                }
            }

            // DOI
            if let Some(ref doi) = paper.doi {
                div { style: "margin-bottom: 12px;",
                    label { style: "font-weight: 600; font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.5px;", "DOI" }
                    div { style: "margin-top: 4px; word-break: break-all;", "{doi}" }
                }
            }

            // Abstract
            if let Some(ref abstract_text) = paper.abstract_text {
                div { style: "margin-bottom: 12px;",
                    label { style: "font-weight: 600; font-size: 12px; color: #666; text-transform: uppercase; letter-spacing: 0.5px;", "Abstract" }
                    div { style: "margin-top: 4px; font-size: 13px; line-height: 1.5; color: #444;", "{abstract_text}" }
                }
            }

            // Delete button
            div { style: "margin-top: 24px; padding-top: 16px; border-top: 1px solid #eee;",
                button {
                    style: "padding: 6px 12px; background: #dc2626; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 13px;",
                    onclick: move |_| {
                        let db = db.clone();
                        if let Ok(()) = db.with_conn(|conn| crate::db::papers::delete_paper(conn, paper_id)) {
                            lib_state.with_mut(|s| {
                                s.papers.retain(|p| p.id != Some(paper_id));
                                s.selected_paper_id = None;
                            });
                        }
                    },
                    "Delete Paper"
                }
            }
        }
    }
}

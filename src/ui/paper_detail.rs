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

            // Close button
            div { class: "detail-header",
                h3 { class: "detail-heading", "Details" }
                button {
                    class: "detail-close",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.selected_paper_id = None);
                    },
                    "\u{00d7}"
                }
            }

            // Title
            div { class: "detail-field",
                label { class: "detail-label", "Title" }
                div { class: "detail-value detail-value--title", "{paper.title}" }
            }

            // Authors
            div { class: "detail-field",
                label { class: "detail-label", "Authors" }
                div { class: "detail-value", "{authors_display}" }
            }

            // Year
            if let Some(year) = paper.year {
                div { class: "detail-field",
                    label { class: "detail-label", "Year" }
                    div { class: "detail-value", "{year}" }
                }
            }

            // Journal
            if let Some(ref journal) = paper.journal {
                div { class: "detail-field",
                    label { class: "detail-label", "Journal" }
                    div { class: "detail-value detail-value--journal", "{journal}" }
                }
            }

            // DOI
            if let Some(ref doi) = paper.doi {
                div { class: "detail-field",
                    label { class: "detail-label", "DOI" }
                    div { class: "detail-value detail-value--doi", "{doi}" }
                }
            }

            // Abstract
            if let Some(ref abstract_text) = paper.abstract_text {
                div { class: "detail-field",
                    label { class: "detail-label", "Abstract" }
                    div { class: "detail-value detail-value--abstract", "{abstract_text}" }
                }
            }

            // Delete button
            div { class: "detail-delete-section",
                button {
                    class: "btn btn--danger",
                    onclick: move |_| {
                        let db = db.clone();
                        spawn(async move {
                            if let Ok(()) = crate::db::papers::delete_paper(db.conn(), paper_id).await {
                                lib_state.with_mut(|s| {
                                    s.papers.retain(|p| p.id != Some(paper_id));
                                    s.selected_paper_id = None;
                                });
                            }
                        });
                    },
                    "Delete Paper"
                }
            }
        }
    }
}

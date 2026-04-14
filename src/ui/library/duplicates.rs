use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;
use rotero_models::Paper;

#[component]
pub fn DuplicatesView(groups: Vec<Vec<Paper>>) -> Element {
    let db = use_context::<Database>();
    let mut lib_state = use_context::<Signal<LibraryState>>();

    rsx! {
        if !groups.is_empty() {
            {
                let group_count_label = if groups.len() == 1 {
                    "1 duplicate group".to_string()
                } else {
                    format!("{} duplicate groups", groups.len())
                };
                let all_groups = groups.clone();
                rsx! {
                    div { class: "duplicate-merge-all",
                        button {
                            class: "btn btn--sm btn--primary",
                            title: "Auto-resolve all groups by keeping the paper with the most metadata",
                            onclick: {
                                let db = db.clone();
                                move |_| {
                                    let db = db.clone();
                                    let groups = all_groups.clone();
                                    spawn(async move {
                                        for group in &groups {
                                            let best = group.iter().max_by_key(|p| p.metadata_completeness_score());
                                            if let Some(best) = best {
                                                let keep_id = best.id.clone().unwrap_or_default();
                                                for p in group {
                                                    if let Some(ref id) = p.id
                                                        && *id != keep_id {
                                                            let _ = rotero_db::papers::merge_papers(db.conn(), &keep_id, id).await;
                                                        }
                                                }
                                            }
                                        }
                                        crate::state::commands::refresh_papers_and_duplicates(db.conn(), &mut lib_state).await;
                                    });
                                }
                            },
                            "Merge All (Keep Best)"
                        }
                        span { class: "duplicate-merge-all-hint",
                            "{group_count_label}"
                        }
                    }
                }
            }
        }
        for (gi, group) in groups.iter().enumerate() {
            {
                let group_key = format!("dup-group-{gi}");
                let reason = if group.len() >= 2 && group[0].doi.is_some() && group[0].doi == group[1].doi {
                    format!("Shared DOI: {}", group[0].doi.as_deref().unwrap_or(""))
                } else {
                    "Similar title".to_string()
                };
                rsx! {
                    div { key: "{group_key}", class: "duplicate-group",
                        div { class: "duplicate-group-header",
                            span { class: "duplicate-group-reason", "{reason}" }
                            span { class: "duplicate-group-count", "{group.len()} papers" }
                        }
                        for paper in group.iter() {
                            {
                                let pid = paper.id.clone().unwrap_or_default();
                                let title = paper.title.clone();
                                let authors = paper.formatted_authors();
                                let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                                let has_pdf = paper.links.pdf_path.is_some();
                                let doi_display = paper.doi.clone().unwrap_or_default();
                                let journal = paper.publication.journal.clone().unwrap_or_default();
                                let date_added = paper.status.date_added.format("%Y-%m-%d").to_string();
                                let field_count = paper.metadata_completeness_score() as usize;
                                rsx! {
                                    div { class: "duplicate-item",
                                        div { class: "duplicate-item-info",
                                            div { class: "duplicate-item-title-row",
                                                div { class: "library-card-title", "{title}" }
                                                if has_pdf {
                                                    span { class: "duplicate-pdf-badge", "PDF" }
                                                }
                                            }
                                            div { class: "library-card-meta",
                                                span { class: "library-card-authors", "{authors}" }
                                                if !year.is_empty() {
                                                    span { class: "library-card-sep", "\u{00b7}" }
                                                    span { class: "library-card-year", "{year}" }
                                                }
                                                if !journal.is_empty() {
                                                    span { class: "library-card-sep", "\u{00b7}" }
                                                    span { class: "duplicate-journal", "{journal}" }
                                                }
                                            }
                                            div { class: "duplicate-item-details",
                                                if !doi_display.is_empty() {
                                                    span { class: "duplicate-doi", "DOI: {doi_display}" }
                                                }
                                                span { class: "duplicate-date-added", "Added: {date_added}" }
                                                span { class: "duplicate-field-count", "{field_count}/7 fields" }
                                            }
                                        }
                                        div { class: "duplicate-item-actions",
                                            button {
                                                class: "btn btn--sm btn--primary",
                                                title: "Keep this paper and merge others into it",
                                                onclick: {
                                                    let db = db.clone();
                                                    let other_ids: Vec<String> = group.iter()
                                                        .filter_map(|p| p.id.clone())
                                                        .filter(|id| *id != pid)
                                                        .collect();
                                                    let pid2 = pid.clone();
                                                    move |_| {
                                                        let db = db.clone();
                                                        let other_ids = other_ids.clone();
                                                        let pid = pid2.clone();
                                                        spawn(async move {
                                                            for del_id in &other_ids {
                                                                let _ = rotero_db::papers::merge_papers(db.conn(), &pid, del_id).await;
                                                            }
                                                            crate::state::commands::refresh_papers_and_duplicates(db.conn(), &mut lib_state).await;
                                                        });
                                                    }
                                                },
                                                "Keep"
                                            }
                                            button {
                                                class: "btn btn--sm btn--danger",
                                                title: "Delete this paper without merging",
                                                onclick: {
                                                    let db = db.clone();
                                                    let pid2 = pid.clone();
                                                    move |_| {
                                                        let db = db.clone();
                                                        let pid = pid2.clone();
                                                        spawn(async move {
                                                            let _ = rotero_db::papers::delete_paper(db.conn(), &pid).await;
                                                            crate::state::commands::refresh_papers_and_duplicates(db.conn(), &mut lib_state).await;
                                                        });
                                                    }
                                                },
                                                "Delete"
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

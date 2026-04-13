use dioxus::prelude::*;

use crate::state::app_state::{DragPaper, LibraryState, PdfTabManager};
use rotero_db::Database;
use rotero_models::Paper;

/// Renders a single paper row in the library list.
#[component]
pub fn PaperCard(
    paper: Paper,
    selected: bool,
    ctx_menu: Signal<Option<(String, f64, f64)>>,
) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let db = use_context::<Database>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();
    let mut drag_paper = use_context::<Signal<DragPaper>>();
    let filtered_ids = use_context::<Signal<Vec<String>>>();

    let paper_id = paper.id.clone().unwrap_or_default();
    let title = paper.title.clone();
    let pdf_rel_path = paper.links.pdf_path.clone();
    let authors = paper.formatted_authors();
    let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
    let journal = paper.publication.journal.clone().unwrap_or_default();
    let citation_count = paper.citation.citation_count;
    let has_pdf = paper.links.pdf_path.is_some();
    let is_read = paper.status.is_read;
    let is_fav = paper.status.is_favorite;

    let row_class = if selected {
        "library-card library-card--selected"
    } else {
        "library-card"
    };

    let pid_drag = paper_id.clone();
    let pid_sel = paper_id.clone();
    let pid_ctx = paper_id.clone();
    let pid_fav = paper_id.clone();
    let pid_open = paper_id.clone();
    let db_for_view = db.clone();
    let db_for_fav = db.clone();

    rsx! {
        div {
            key: "{paper_id}",
            class: "{row_class}",
            draggable: "true",
            ondragstart: move |_| {
                let state = lib_state.read();
                let ids = if state.is_selected(&pid_drag) {
                    state.selected_paper_ids.iter().cloned().collect()
                } else {
                    vec![pid_drag.clone()]
                };
                drop(state);
                drag_paper.set(DragPaper(Some(ids)));
            },
            ondragend: move |evt: Event<DragData>| {
                let _ = evt;
                spawn(async move {
                    drag_paper.set(DragPaper(None));
                });
            },
            onmouseup: move |evt: Event<MouseData>| {
                if drag_paper().0.is_none()
                    && evt.trigger_button() == Some(dioxus::html::input_data::MouseButton::Primary) {
                        let pid = pid_sel.clone();
                        let modifiers = evt.modifiers();
                        let cmd = modifiers.meta() || modifiers.ctrl();
                        let shift = modifiers.shift();
                        if cmd {
                            lib_state.with_mut(|s| s.toggle_select(&pid));
                        } else if shift {
                            lib_state.with_mut(|s| s.range_select(&pid, &filtered_ids()));
                        } else {
                            lib_state.with_mut(|s| s.select_one(pid));
                        }
                    }
            },
            oncontextmenu: move |evt| {
                evt.prevent_default();
                let coords = evt.client_coordinates();
                lib_state.with_mut(|s| {
                    if !s.is_selected(&pid_ctx) {
                        s.select_one(pid_ctx.clone());
                    }
                });
                ctx_menu.set(Some((pid_ctx.clone(), coords.x, coords.y)));
            },

            div { class: "library-card-indicator",
                if !is_read {
                    div { class: "library-unread-dot" }
                }
            }

            div { class: "library-card-body",
                div { class: "library-card-title", "{title}" }
                div { class: "library-card-meta",
                    span { class: "library-card-authors", "{authors}" }
                    if !year.is_empty() {
                        span { class: "library-card-sep", "\u{00b7}" }
                        span { class: "library-card-year", "{year}" }
                    }
                    if !journal.is_empty() {
                        span { class: "library-card-sep", "\u{00b7}" }
                        span { class: "library-card-journal", "{journal}" }
                    }
                    if let Some(count) = citation_count {
                        span { class: "library-card-sep", "\u{00b7}" }
                        span { class: "library-card-citations", title: "Citation count", "{count} cited" }
                    }
                }
            }

            div { class: "library-card-actions",
                button {
                    class: if is_fav { "library-fav-btn library-fav-btn--active" } else { "library-fav-btn" },
                    title: if is_fav { "Unfavorite" } else { "Favorite" },
                    onclick: {
                        let pid = pid_fav.clone();
                        move |evt: Event<MouseData>| {
                        evt.stop_propagation();
                        let db = db_for_fav.clone();
                        let new_val = !is_fav;
                        let pid = pid.clone();
                        spawn(async move {
                            if let Ok(()) = rotero_db::papers::set_favorite(db.conn(), &pid, new_val).await {
                                let pid2 = pid.clone();
                                lib_state.with_mut(|s| {
                                    if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                        p.status.is_favorite = new_val;
                                    }
                                });
                            }
                        });
                    }},
                    i { class: if is_fav { "bi bi-star-fill" } else { "bi bi-star" } }
                }

                if has_pdf {
                    button {
                        class: "btn btn--ghost",
                        onclick: move |evt| {
                            evt.stop_propagation();
                            if let Some(ref rel_path) = pdf_rel_path {
                                crate::state::commands::open_paper_pdf(&db_for_view, &mut tabs, &mut lib_state, &config, &dpr_sig, &pid_open, rel_path, &title);
                            }
                        },
                        "Open"
                    }
                }
            }
        }
    }
}

use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfViewState};
use super::search_bar::SearchBar;
use super::import_export::ImportExportButtons;

#[component]
pub fn LibraryPanel() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let db = use_context::<Database>();
    let state = lib_state.read();

    let is_searching = state.search_results.is_some();

    let filtered: Vec<_> = if is_searching {
        state.search_results.as_ref().unwrap().clone()
    } else {
        match &state.view {
            LibraryView::AllPapers => state.papers.clone(),
            LibraryView::RecentlyAdded => {
                let mut p = state.papers.clone();
                p.sort_by(|a, b| b.date_added.cmp(&a.date_added));
                p.truncate(20);
                p
            }
            LibraryView::Favorites => state.papers.iter().filter(|p| p.is_favorite).cloned().collect(),
            LibraryView::Unread => state.papers.iter().filter(|p| !p.is_read).cloned().collect(),
            _ => state.papers.clone(),
        }
    };

    let view_title = if is_searching {
        "Search Results"
    } else {
        match &state.view {
            LibraryView::AllPapers => "All Papers",
            LibraryView::RecentlyAdded => "Recently Added",
            LibraryView::Favorites => "Favorites",
            LibraryView::Unread => "Unread",
            LibraryView::Collection(_) => "Collection",
            _ => "Papers",
        }
    };

    let paper_count = filtered.len();

    rsx! {
        div { class: "library-view",

            // Header
            div { class: "library-header",
                div { class: "library-header-left",
                    h2 { class: "library-title", "{view_title}" }
                    span { class: "library-count", "{paper_count} papers" }
                }
                div { class: "library-header-right",
                    ImportExportButtons {}
                    AddPaperButton {}
                }
            }

            // Search bar
            SearchBar {}

            // Paper list
            div { class: "library-list",
                if filtered.is_empty() {
                    div { class: "library-empty",
                        if is_searching {
                            p { class: "library-empty-heading", "No results found" }
                            p { class: "library-empty-sub", "Try a different search term." }
                        } else {
                            p { class: "library-empty-heading", "No papers yet" }
                            p { class: "library-empty-sub", "Click \"Add Paper\" or use the browser connector to import papers." }
                        }
                    }
                } else {
                    for paper in filtered.iter() {
                        {
                            let paper_id = paper.id.unwrap_or(0);
                            let title = paper.title.clone();
                            let pdf_rel_path = paper.pdf_path.clone();
                            let authors = if paper.authors.is_empty() {
                                "Unknown".to_string()
                            } else if paper.authors.len() <= 2 {
                                paper.authors.join(", ")
                            } else {
                                format!("{} et al.", paper.authors[0])
                            };
                            let year = paper.year.map(|y| y.to_string()).unwrap_or_default();
                            let journal = paper.journal.clone().unwrap_or_default();
                            let has_pdf = paper.pdf_path.is_some();
                            let is_read = paper.is_read;
                            let is_fav = paper.is_favorite;
                            let selected = state.selected_paper_id == Some(paper_id);
                            let row_class = if selected {
                                "library-card library-card--selected"
                            } else {
                                "library-card"
                            };
                            let db_for_view = db.clone();
                            let db_for_fav = db.clone();

                            rsx! {
                                div {
                                    key: "{paper_id}",
                                    class: "{row_class}",
                                    onclick: move |_| {
                                        lib_state.with_mut(|s| {
                                            s.selected_paper_id = Some(paper_id);
                                        });
                                    },

                                    // Left: read indicator
                                    div { class: "library-card-indicator",
                                        if !is_read {
                                            div { class: "library-unread-dot" }
                                        }
                                    }

                                    // Center: paper info
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
                                        }
                                    }

                                    // Right: actions
                                    div { class: "library-card-actions",
                                        // Favorite toggle
                                        button {
                                            class: if is_fav { "library-fav-btn library-fav-btn--active" } else { "library-fav-btn" },
                                            title: if is_fav { "Unfavorite" } else { "Favorite" },
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                let db = db_for_fav.clone();
                                                let new_val = !is_fav;
                                                spawn(async move {
                                                    if let Ok(()) = crate::db::papers::set_favorite(db.conn(), paper_id, new_val).await {
                                                        lib_state.with_mut(|s| {
                                                            if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
                                                                p.is_favorite = new_val;
                                                            }
                                                        });
                                                    }
                                                });
                                            },
                                            i { class: if is_fav { "bi bi-star-fill" } else { "bi bi-star" } }
                                        }

                                        // View PDF
                                        if has_pdf {
                                            button {
                                                class: "btn btn--ghost",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if let Some(ref rel_path) = pdf_rel_path {
                                                        let full_path = db_for_view.resolve_pdf_path(rel_path);
                                                        let path_str = full_path.to_string_lossy().to_string();
                                                        if let Ok(engine) = rotero_pdf::PdfEngine::new(None) {
                                                            if crate::state::commands::open_pdf(&engine, &mut pdf_state, &path_str).is_ok() {
                                                                pdf_state.with_mut(|s| s.paper_id = Some(paper_id));
                                                                let db_clone = db_for_view.clone();
                                                                spawn(async move {
                                                                    if let Ok(anns) = crate::db::annotations::list_annotations_for_paper(db_clone.conn(), paper_id).await {
                                                                        pdf_state.with_mut(|s| s.annotations = anns);
                                                                    }
                                                                });
                                                                lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                                            }
                                                        }
                                                    }
                                                },
                                                "Open"
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
fn AddPaperButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<crate::db::Database>();
    let mut error_msg = use_signal(|| None::<String>);
    let mut show_doi_input = use_signal(|| false);
    let mut doi_value = use_signal(|| String::new());

    let db_for_pdf = db.clone();
    let db_for_doi = db.clone();

    rsx! {
        div { class: "add-paper-row",
            button {
                class: "btn btn--primary",
                onclick: move |_| {
                    let file = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_title("Add Paper PDF")
                        .pick_file();

                    if let Some(path) = file {
                        let path_str = path.to_string_lossy().to_string();
                        let db = db_for_pdf.clone();

                        let filename = path.file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Untitled".to_string());

                        match db.import_pdf(&path_str, Some(&filename), None, None) {
                            Ok(rel_path) => {
                                let mut paper = rotero_models::Paper::new(filename);
                                paper.pdf_path = Some(rel_path);

                                spawn(async move {
                                    match crate::db::papers::insert_paper(db.conn(), &paper).await {
                                        Ok(id) => {
                                            paper.id = Some(id);
                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                            error_msg.set(None);
                                        }
                                        Err(e) => error_msg.set(Some(format!("{e}"))),
                                    }
                                });
                            }
                            Err(e) => error_msg.set(Some(e)),
                        }
                    }
                },
                "+ Add PDF"
            }

            button {
                class: "btn btn--success",
                onclick: move |_| {
                    show_doi_input.set(!show_doi_input());
                },
                "+ DOI"
            }
        }

        if show_doi_input() {
            div { class: "doi-input-row",
                input {
                    class: "doi-input",
                    r#type: "text",
                    placeholder: "Enter DOI (e.g. 10.1234/...)",
                    value: "{doi_value}",
                    oninput: move |evt| doi_value.set(evt.value()),
                }
                button {
                    class: "btn btn--success",
                    onclick: move |_| {
                        let doi = doi_value().trim().to_string();
                        if doi.is_empty() {
                            return;
                        }
                        let db = db_for_doi.clone();

                        spawn(async move {
                            match crate::metadata::crossref::fetch_by_doi(&doi).await {
                                Ok(meta) => {
                                    let paper = crate::metadata::parser::metadata_to_paper(meta);
                                    match crate::db::papers::insert_paper(db.conn(), &paper).await {
                                        Ok(id) => {
                                            let mut paper = paper;
                                            paper.id = Some(id);
                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                            show_doi_input.set(false);
                                            doi_value.set(String::new());
                                            error_msg.set(None);
                                        }
                                        Err(e) => error_msg.set(Some(format!("{e}"))),
                                    }
                                }
                                Err(e) => error_msg.set(Some(e)),
                            }
                        });
                    },
                    "Fetch"
                }
            }
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { class: "error-message",
                "{err}"
            }
        }
    }
}

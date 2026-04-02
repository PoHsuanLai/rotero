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

    let display_papers = state.search_results.as_ref().unwrap_or(&state.papers);
    let is_searching = state.search_results.is_some();

    rsx! {
        div { class: "library-view",

            // Header
            div { class: "library-header",
                h2 { class: "library-title",
                    if is_searching { "Search Results" } else { "All Papers" }
                }
                ImportExportButtons {}
                AddPaperButton {}
            }

            // Search bar
            SearchBar {}

            // Paper list
            div { class: "library-table-scroll",
                if display_papers.is_empty() {
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
                    // Table header
                    div { class: "library-table-header",
                        span { "Title" }
                        span { "Authors" }
                        span { "Year" }
                        span { "Actions" }
                    }
                    // Paper rows
                    for paper in display_papers.iter() {
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
                            let has_pdf = paper.pdf_path.is_some();
                            let selected = state.selected_paper_id == Some(paper_id);
                            let row_class = if selected {
                                "library-row library-row--selected"
                            } else {
                                "library-row"
                            };
                            let db_for_view = db.clone();

                            rsx! {
                                div {
                                    key: "{paper_id}",
                                    class: "{row_class}",
                                    onclick: move |_| {
                                        lib_state.with_mut(|s| {
                                            s.selected_paper_id = Some(paper_id);
                                        });
                                    },
                                    span { class: "library-paper-title", "{title}" }
                                    span { class: "library-paper-authors", "{authors}" }
                                    span { class: "library-paper-year", "{year}" }
                                    div { class: "library-actions",
                                        if has_pdf {
                                            button {
                                                class: "btn btn--ghost",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    if let Some(ref rel_path) = pdf_rel_path {
                                                        let full_path = db_for_view.pdfs_dir().join(rel_path);
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
                                                "View"
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

                        match db.import_pdf(&path_str) {
                            Ok(rel_path) => {
                                let filename = path.file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Untitled".to_string());

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

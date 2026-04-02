use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, LibraryView};

#[component]
pub fn LibraryPanel() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let state = lib_state.read();

    rsx! {
        div { class: "library-view",
            style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",

            // Header
            div { style: "padding: 16px; border-bottom: 1px solid #eee; display: flex; align-items: center; gap: 12px;",
                h2 { style: "margin: 0; font-size: 18px; flex: 1;", "All Papers" }
                AddPaperButton {}
            }

            // Paper list
            div { style: "flex: 1; overflow-y: auto;",
                if state.papers.is_empty() {
                    div { style: "padding: 40px; text-align: center; color: #999;",
                        p { style: "font-size: 16px; margin-bottom: 8px;", "No papers yet" }
                        p { style: "font-size: 14px;", "Click \"Add Paper\" or use the browser connector to import papers." }
                    }
                } else {
                    // Table header
                    div { style: "display: grid; grid-template-columns: 1fr 200px 60px 100px; padding: 8px 16px; border-bottom: 1px solid #eee; font-size: 12px; color: #999; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px;",
                        span { "Title" }
                        span { "Authors" }
                        span { "Year" }
                        span { "Actions" }
                    }
                    // Paper rows
                    for paper in state.papers.iter() {
                        {
                            let paper_id = paper.id.unwrap_or(0);
                            let title = paper.title.clone();
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
                            let bg = if selected { "#e8f4fd" } else { "transparent" };

                            rsx! {
                                div {
                                    key: "{paper_id}",
                                    style: "display: grid; grid-template-columns: 1fr 200px 60px 100px; padding: 10px 16px; border-bottom: 1px solid #f0f0f0; cursor: pointer; background: {bg}; font-size: 14px; align-items: center;",
                                    onclick: move |_| {
                                        lib_state.with_mut(|s| {
                                            s.selected_paper_id = Some(paper_id);
                                        });
                                    },
                                    span { style: "font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{title}" }
                                    span { style: "color: #666; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;", "{authors}" }
                                    span { style: "color: #666;", "{year}" }
                                    div { style: "display: flex; gap: 4px;",
                                        if has_pdf {
                                            button {
                                                style: "padding: 2px 8px; border: 1px solid #ddd; background: #fff; border-radius: 4px; cursor: pointer; font-size: 12px;",
                                                onclick: move |evt| {
                                                    evt.stop_propagation();
                                                    lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
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
        div { style: "display: flex; gap: 8px; align-items: center;",
            // Add PDF button
            button {
                style: "padding: 6px 12px; background: #2563eb; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 13px;",
                onclick: move |_| {
                    let file = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_title("Add Paper PDF")
                        .pick_file();

                    if let Some(path) = file {
                        let path_str = path.to_string_lossy().to_string();
                        let db = db_for_pdf.clone();

                        // Import PDF and create paper entry
                        match db.import_pdf(&path_str) {
                            Ok(rel_path) => {
                                let filename = path.file_stem()
                                    .map(|s| s.to_string_lossy().to_string())
                                    .unwrap_or_else(|| "Untitled".to_string());

                                let mut paper = rotero_models::Paper::new(filename);
                                paper.pdf_path = Some(rel_path);

                                match db.with_conn(|conn| crate::db::papers::insert_paper(conn, &paper)) {
                                    Ok(id) => {
                                        paper.id = Some(id);
                                        lib_state.with_mut(|s| s.papers.insert(0, paper));
                                        error_msg.set(None);
                                    }
                                    Err(e) => error_msg.set(Some(e)),
                                }
                            }
                            Err(e) => error_msg.set(Some(e)),
                        }
                    }
                },
                "+ Add PDF"
            }

            // Add by DOI button
            button {
                style: "padding: 6px 12px; background: #059669; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 13px;",
                onclick: move |_| {
                    show_doi_input.set(!show_doi_input());
                },
                "+ DOI"
            }
        }

        // DOI input form
        if show_doi_input() {
            div { style: "display: flex; gap: 8px; margin-top: 8px;",
                input {
                    style: "flex: 1; padding: 6px 10px; border: 1px solid #ddd; border-radius: 4px; font-size: 13px;",
                    r#type: "text",
                    placeholder: "Enter DOI (e.g. 10.1234/...)",
                    value: "{doi_value}",
                    oninput: move |evt| doi_value.set(evt.value()),
                }
                button {
                    style: "padding: 6px 12px; background: #059669; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 13px;",
                    onclick: move |_| {
                        let doi = doi_value().trim().to_string();
                        if doi.is_empty() {
                            return;
                        }
                        let db = db_for_doi.clone();

                        // Spawn async DOI fetch
                        spawn(async move {
                            match crate::metadata::crossref::fetch_by_doi(&doi).await {
                                Ok(meta) => {
                                    let paper = crate::metadata::parser::metadata_to_paper(meta);
                                    match db.with_conn(|conn| crate::db::papers::insert_paper(conn, &paper)) {
                                        Ok(id) => {
                                            let mut paper = paper;
                                            paper.id = Some(id);
                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                            show_doi_input.set(false);
                                            doi_value.set(String::new());
                                            error_msg.set(None);
                                        }
                                        Err(e) => error_msg.set(Some(e)),
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
            div { style: "margin-top: 8px; padding: 6px 10px; background: #fee; border-radius: 4px; color: #c00; font-size: 12px;",
                "{err}"
            }
        }
    }
}

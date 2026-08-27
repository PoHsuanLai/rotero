use dioxus::prelude::*;

use crate::state::app_state::LibraryState;

#[component]
pub(crate) fn AddPaperButtons() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<rotero_db::Database>();
    let render_ch = use_context::<crate::app::RenderChannel>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let mut error_msg = use_context::<Signal<Option<String>>>();
    let mut show_doi_input = use_context::<Signal<bool>>();

    let db_for_pdf = db.clone();

    rsx! {
        div { class: "add-paper-row",
            button {
                class: "btn btn--primary",
                onclick: move |_| {
                    let db_for_pdf = db_for_pdf.clone();
                    spawn(async move {
                        let file = crate::ui::pick_file_async(&["pdf"], "Add Paper PDF").await;

                        if let Some(path) = file {
                            let path_str = path.to_string_lossy().to_string();
                            let db = db_for_pdf;

                            let filename = path.file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Untitled".to_string());

                            match db.import_pdf(&path_str, Some(&filename), None, None) {
                                Ok((rel_path, sha256)) => {
                                    let mut paper = rotero_models::Paper {
                                        title: filename,
                                        links: rotero_models::PaperLinks {
                                            pdf_path: Some(rel_path.clone()),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    };
                                    let full_path = db.resolve_pdf_path(&rel_path).to_string_lossy().to_string();
                                    let auto_fetch = config.read().auto_fetch_metadata;
                                    let meta_render_tx = render_ch.sender();
                                    let meta_db = db.clone();

                                    match db.insert_paper(&paper).await {
                                        Ok(id) => {
                                            // The model carries no hash field, so
                                            // record it right after the insert —
                                            // a synced `pdf_path` without one
                                            // cannot be published to peers.
                                            let _ = db
                                                .update_pdf_path(&id, &rel_path, Some(&sha256))
                                                .await;
                                            paper.id = Some(id.clone());
                                            lib_state.with_mut(|s| s.papers.insert(0, paper));
                                            error_msg.set(None);
                                            spawn(async move {
                                                crate::state::commands::extract_and_fetch_metadata(
                                                    &meta_render_tx, &meta_db, &id, &full_path, auto_fetch, &mut lib_state,
                                                ).await;
                                            });
                                        }
                                        Err(e) => error_msg.set(Some(format!("{e}"))),
                                    }
                                }
                                Err(e) => error_msg.set(Some(e)),
                            }
                        }
                    });
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
    }
}

#[component]
pub(crate) fn AddPaperDOIInput() -> Element {
    let import_channel = use_context::<crate::state::commands::ImportChannel>();
    let mut error_msg = use_context::<Signal<Option<String>>>();
    let mut show_doi_input = use_context::<Signal<bool>>();
    let mut doi_value = use_signal(String::new);

    rsx! {
        if show_doi_input() {
            div { class: "doi-input-row",
                input {
                    class: "input doi-input",
                    r#type: "text",
                    placeholder: "Enter DOI (e.g. 10.1234/...)",
                    value: "{doi_value}",
                    onfocusin: crate::ui::keybindings::editable_focus_in,
                    onfocusout: crate::ui::keybindings::editable_focus_out,
                    oninput: move |evt| doi_value.set(evt.value()),
                }
                button {
                    class: "btn btn--success",
                    onclick: move |_| {
                        let doi = doi_value().trim().to_string();
                        if doi.is_empty() {
                            return;
                        }
                        // Fetch metadata, then hand off to the import coroutine,
                        // which inserts and downloads the OA PDF (same path as a
                        // web-result import) — outliving this input closing.
                        spawn(async move {
                            match crate::metadata::crossref::fetch_by_doi(&doi).await {
                                Ok(paper) => {
                                    import_channel.import(paper);
                                    show_doi_input.set(false);
                                    doi_value.set(String::new());
                                    error_msg.set(None);
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

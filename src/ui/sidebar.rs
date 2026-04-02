use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfViewState};

#[component]
pub fn Sidebar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let state = lib_state.read();
    let view = state.view.clone();
    let all_papers_bg = if view == LibraryView::AllPapers { "#e0e0e0" } else { "transparent" };
    let paper_count = state.papers.len();

    rsx! {
        div { class: "sidebar",
            style: "width: 250px; background: #f5f5f5; border-right: 1px solid #ddd; padding: 16px; overflow-y: auto; display: flex; flex-direction: column;",
            h2 { style: "margin: 0 0 16px 0; font-size: 18px;", "Rotero" }

            // Open PDF button (standalone viewer)
            OpenPdfButton {}

            // Library navigation
            div { style: "margin-top: 20px;",
                // All Papers
                div {
                    style: "padding: 6px 8px; cursor: pointer; border-radius: 4px; font-size: 14px; font-weight: 500; background: {all_papers_bg};",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                    },
                    "All Papers ({paper_count})"
                }
            }

            // Collections
            div { style: "margin-top: 16px;",
                div { style: "display: flex; justify-content: space-between; align-items: center;",
                    h3 { style: "font-size: 13px; color: #666; margin: 0; text-transform: uppercase; letter-spacing: 0.5px;", "Collections" }
                    NewCollectionButton {}
                }
                div { style: "margin-top: 8px;",
                    if state.collections.is_empty() {
                        p { style: "color: #bbb; font-size: 13px; padding: 4px 8px;", "No collections" }
                    } else {
                        for coll in state.collections.iter() {
                            {
                                let coll_id = coll.id.unwrap_or(0);
                                let coll_name = coll.name.clone();
                                rsx! {
                                    div {
                                        key: "{coll_id}",
                                        style: "padding: 4px 8px; cursor: pointer; border-radius: 4px; font-size: 14px;",
                                        onclick: move |_| {
                                            lib_state.with_mut(|s| s.view = LibraryView::Collection(coll_id));
                                        },
                                        "{coll_name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Tags
            div { style: "margin-top: 16px;",
                h3 { style: "font-size: 13px; color: #666; margin: 0 0 8px 0; text-transform: uppercase; letter-spacing: 0.5px;", "Tags" }
                if state.tags.is_empty() {
                    p { style: "color: #bbb; font-size: 13px; padding: 4px 8px;", "No tags" }
                } else {
                    div { style: "display: flex; flex-wrap: wrap; gap: 4px; padding: 4px 8px;",
                        for tag in state.tags.iter() {
                            {
                                let tag_name = tag.name.clone();
                                let bg = tag.color.clone().unwrap_or_else(|| "#e0e0e0".to_string());
                                rsx! {
                                    span {
                                        style: "display: inline-block; padding: 2px 8px; border-radius: 12px; font-size: 12px; background: {bg}; cursor: pointer;",
                                        "{tag_name}"
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
fn NewCollectionButton() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut show_input = use_signal(|| false);
    let mut name_value = use_signal(|| String::new());

    rsx! {
        if show_input() {
            div { style: "display: flex; gap: 4px; margin-top: 4px;",
                input {
                    style: "flex: 1; padding: 4px 6px; border: 1px solid #ddd; border-radius: 4px; font-size: 12px;",
                    r#type: "text",
                    placeholder: "Name",
                    value: "{name_value}",
                    oninput: move |evt| name_value.set(evt.value()),
                    onkeypress: move |evt| {
                        if evt.key() == Key::Enter {
                            let name = name_value().trim().to_string();
                            if !name.is_empty() {
                                let coll = rotero_models::Collection::new(name);
                                let db = db.clone();
                                if let Ok(id) = db.with_conn(|conn| crate::db::collections::insert_collection(conn, &coll)) {
                                    let mut coll = coll;
                                    coll.id = Some(id);
                                    lib_state.with_mut(|s| s.collections.push(coll));
                                }
                                show_input.set(false);
                                name_value.set(String::new());
                            }
                        }
                    },
                }
            }
        } else {
            button {
                style: "padding: 2px 6px; border: none; background: transparent; color: #999; cursor: pointer; font-size: 16px;",
                onclick: move |_| show_input.set(true),
                "+"
            }
        }
    }
}

#[component]
fn OpenPdfButton() -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut error_msg = use_signal(|| None::<String>);

    rsx! {
        button {
            style: "width: 100%; padding: 8px; background: #f0f0f0; color: #333; border: 1px solid #ddd; border-radius: 6px; cursor: pointer; font-size: 13px;",
            onclick: move |_| {
                let file = rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_title("Open PDF")
                    .pick_file();

                if let Some(path) = file {
                    let path_str = path.to_string_lossy().to_string();

                    match rotero_pdf::PdfEngine::new(None) {
                        Ok(engine) => {
                            match crate::state::commands::open_pdf(&engine, &mut pdf_state, &path_str) {
                                Ok(()) => {
                                    lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                    error_msg.set(None);
                                }
                                Err(e) => error_msg.set(Some(format!("Failed to open PDF: {e}"))),
                            }
                        }
                        Err(e) => error_msg.set(Some(format!("PDFium not found: {e}"))),
                    }
                }
            },
            "Open PDF Viewer"
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { style: "margin-top: 4px; padding: 6px; background: #fee; border-radius: 4px; color: #c00; font-size: 11px;",
                "{err}"
            }
        }
    }
}

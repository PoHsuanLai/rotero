use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfViewState};

#[component]
pub fn Sidebar() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let state = lib_state.read();
    let view = state.view.clone();
    let is_all_papers = view == LibraryView::AllPapers;
    let paper_count = state.papers.len();

    let nav_class = if is_all_papers {
        "sidebar-nav-item sidebar-nav-item--active"
    } else {
        "sidebar-nav-item"
    };

    rsx! {
        div { class: "sidebar",
            h2 { class: "sidebar-brand", "Rotero" }

            OpenPdfButton {}

            // Library navigation
            div { class: "sidebar-nav",
                div {
                    class: "{nav_class}",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.view = LibraryView::AllPapers);
                    },
                    "All Papers ({paper_count})"
                }
            }

            // Collections
            div { class: "sidebar-section",
                div { class: "sidebar-section-header",
                    h3 { class: "sidebar-section-title", "Collections" }
                    NewCollectionButton {}
                }
                div { class: "sidebar-section-content",
                    if state.collections.is_empty() {
                        p { class: "sidebar-empty", "No collections" }
                    } else {
                        for coll in state.collections.iter() {
                            {
                                let coll_id = coll.id.unwrap_or(0);
                                let coll_name = coll.name.clone();
                                rsx! {
                                    div {
                                        key: "{coll_id}",
                                        class: "sidebar-collection-item",
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
            div { class: "sidebar-section",
                h3 { class: "sidebar-section-title", "Tags" }
                if state.tags.is_empty() {
                    p { class: "sidebar-empty", "No tags" }
                } else {
                    div { class: "sidebar-tags-wrap",
                        for tag in state.tags.iter() {
                            {
                                let tag_name = tag.name.clone();
                                let bg = tag.color.clone().unwrap_or_else(|| "#6b7085".to_string());
                                rsx! {
                                    span {
                                        class: "sidebar-tag",
                                        style: "background: {bg};",
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
            div { class: "sidebar-input-row",
                input {
                    class: "sidebar-input",
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
                                spawn(async move {
                                    if let Ok(id) = crate::db::collections::insert_collection(db.conn(), &coll).await {
                                        let mut coll = coll;
                                        coll.id = Some(id);
                                        lib_state.with_mut(|s| s.collections.push(coll));
                                    }
                                    show_input.set(false);
                                    name_value.set(String::new());
                                });
                            }
                        }
                    },
                }
            }
        } else {
            button {
                class: "sidebar-add-btn",
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
            class: "sidebar-open-btn",
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
            div { class: "sidebar-error",
                "{err}"
            }
        }
    }
}

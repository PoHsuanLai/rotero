use dioxus::prelude::*;

use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfTab, PdfTabManager};
use crate::sync::engine::SyncConfig;
use super::components::context_menu::{ContextMenu, ContextMenuItem};

#[component]
pub fn PaperDetail() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<SyncConfig>>();

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

    // DOI context menu state: (doi_string, x, y)
    let mut doi_ctx = use_signal(|| None::<(String, f64, f64)>);

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
                {
                    let doi_for_ctx = doi.clone();
                    rsx! {
                        div { class: "detail-field",
                            label { class: "detail-label", "DOI" }
                            div {
                                class: "detail-value detail-value--doi",
                                oncontextmenu: move |evt: Event<MouseData>| {
                                    evt.prevent_default();
                                    doi_ctx.set(Some((doi_for_ctx.clone(), evt.client_coordinates().x, evt.client_coordinates().y)));
                                },
                                "{doi}"
                            }
                        }
                    }
                }
            }

            // Abstract
            if let Some(ref abstract_text) = paper.abstract_text {
                div { class: "detail-field",
                    label { class: "detail-label", "Abstract" }
                    div { class: "detail-value detail-value--abstract", "{abstract_text}" }
                }
            }

            // Add to collection
            div { class: "detail-field",
                label { class: "detail-label", "Collection" }
                AddToCollectionSelect { paper_id }
            }

            // Tags
            div { class: "detail-field",
                label { class: "detail-label", "Tags" }
                TagEditor { paper_id }
            }

            // Citation button
            div { class: "detail-cite-section",
                super::citation_dialog::CitationDialog {}
            }

            // Open / Delete buttons
            div { class: "detail-delete-section",
                div { class: "detail-actions",
                    if paper.pdf_path.is_some() {
                        {
                            let pdf_rel_path = paper.pdf_path.clone();
                            let title = paper.title.clone();
                            let db_open = db.clone();
                            rsx! {
                                button {
                                    class: "btn btn--primary",
                                    onclick: move |_| {
                                        if let Some(ref rel_path) = pdf_rel_path {
                                            let full_path = db_open.resolve_pdf_path(rel_path);
                                            let path_str = full_path.to_string_lossy().to_string();
                                            tabs.with_mut(|m| {
                                                if let Some(idx) = m.find_by_paper_id(paper_id) {
                                                    let tid = m.tabs[idx].id;
                                                    m.switch_to(tid);
                                                } else {
                                                    let cfg = config.read();
                                                    let id = m.next_id();
                                                    let mut tab = PdfTab::new(id, path_str.clone(), title.clone(), cfg.default_zoom, cfg.page_batch_size);
                                                    tab.paper_id = Some(paper_id);
                                                    m.open_tab(tab);
                                                }
                                            });
                                            lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                        }
                                    },
                                    "Open Paper"
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn--danger",
                        onclick: {
                            let db_del = db.clone();
                            move |_| {
                                let db = db_del.clone();
                                spawn(async move {
                                    if let Ok(()) = crate::db::papers::delete_paper(db.conn(), paper_id).await {
                                        lib_state.with_mut(|s| {
                                            s.papers.retain(|p| p.id != Some(paper_id));
                                            s.selected_paper_id = None;
                                        });
                                    }
                                });
                            }
                        },
                        "Delete Paper"
                    }
                }
            }

            // DOI context menu
            if let Some((doi_str, mx, my)) = doi_ctx() {
                {
                    let doi_copy = doi_str.clone();
                    let doi_open = doi_str.clone();
                    rsx! {
                        ContextMenu {
                            x: mx,
                            y: my,
                            on_close: move |_| {
                                doi_ctx.set(None);
                            },

                            ContextMenuItem {
                                label: "Copy DOI".to_string(),
                                icon: Some("bi-clipboard".to_string()),
                                on_click: move |_| {
                                    let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&doi_copy).unwrap_or_default());
                                    let _ = document::eval(&js);
                                    doi_ctx.set(None);
                                },
                            }

                            ContextMenuItem {
                                label: "Open in browser".to_string(),
                                icon: Some("bi-box-arrow-up-right".to_string()),
                                on_click: move |_| {
                                    let url = format!("https://doi.org/{}", doi_open);
                                    let js = format!("window.open({}, '_blank')", serde_json::to_string(&url).unwrap_or_default());
                                    let _ = document::eval(&js);
                                    doi_ctx.set(None);
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn AddToCollectionSelect(paper_id: i64) -> Element {
    let lib_state = use_context::<Signal<crate::state::app_state::LibraryState>>();
    let db = use_context::<Database>();
    let collections = lib_state.read().collections.clone();

    rsx! {
        select {
            class: "select",
            onchange: move |evt| {
                let val = evt.value();
                if val.is_empty() { return; }
                if let Ok(coll_id) = val.parse::<i64>() {
                    let db = db.clone();
                    spawn(async move {
                        let _ = crate::db::collections::add_paper_to_collection(db.conn(), paper_id, coll_id).await;
                    });
                }
            },
            option { value: "", "Add to collection..." }
            for coll in collections.iter() {
                {
                    let cid = coll.id.unwrap_or(0);
                    let cname = coll.name.clone();
                    rsx! { option { value: "{cid}", "{cname}" } }
                }
            }
        }
    }
}

#[component]
fn TagEditor(paper_id: i64) -> Element {
    let mut lib_state = use_context::<Signal<crate::state::app_state::LibraryState>>();
    let db = use_context::<Database>();
    let mut new_tag = use_signal(|| String::new());

    rsx! {
        div { class: "tag-editor",
            input {
                id: "tag-editor-input",
                class: "input input--sm",
                r#type: "text",
                placeholder: "Add tag...",
                value: "{new_tag}",
                oninput: move |evt| new_tag.set(evt.value()),
                onkeypress: move |evt| {
                    if evt.key() == Key::Enter {
                        let tag_name = new_tag().trim().to_string();
                        if tag_name.is_empty() { return; }
                        let db = db.clone();
                        spawn(async move {
                            if let Ok(tag_id) = crate::db::tags::get_or_create_tag(db.conn(), &tag_name, None).await {
                                let _ = crate::db::tags::add_tag_to_paper(db.conn(), paper_id, tag_id).await;
                                // Reload tags
                                if let Ok(tags) = crate::db::tags::list_tags(db.conn()).await {
                                    lib_state.with_mut(|s| s.tags = tags);
                                }
                            }
                            new_tag.set(String::new());
                        });
                    }
                },
            }
        }
    }
}

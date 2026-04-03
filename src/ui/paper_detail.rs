use dioxus::prelude::*;

use super::components::context_menu::{ContextMenu, ContextMenuItem};
use crate::db::Database;
use crate::state::app_state::{LibraryState, LibraryView, PdfTab, PdfTabManager};
use crate::sync::engine::SyncConfig;

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

            // Citation count
            if let Some(count) = paper.citation_count {
                div { class: "detail-field",
                    label { class: "detail-label", "Citations" }
                    div { class: "detail-value detail-value--citations", "{count}" }
                }
            }

            // Citation key
            if let Some(ref cite_key) = paper.citation_key {
                {
                    let key_for_copy = cite_key.clone();
                    let key_for_copy2 = cite_key.clone();
                    let key_display = cite_key.clone();
                    let mut editing_key = use_signal(|| false);
                    let mut edit_key_value = use_signal(|| cite_key.clone());
                    let mut copied_hint = use_signal(|| false);
                    let db_key = db.clone();
                    let db_key2 = db.clone();

                    rsx! {
                        div { class: "detail-field",
                            label { class: "detail-label", "Citation Key" }
                            if editing_key() {
                                div { class: "detail-cite-key-edit",
                                    input {
                                        class: "input input--sm",
                                        value: "{edit_key_value}",
                                        autofocus: true,
                                        oninput: move |evt| edit_key_value.set(evt.value()),
                                        onkeypress: {
                                            let db = db_key.clone();
                                            move |evt: Event<KeyboardData>| {
                                                if evt.key() == Key::Enter {
                                                    let new_key = edit_key_value().trim().to_string();
                                                    if !new_key.is_empty() {
                                                        let db = db.clone();
                                                        spawn(async move {
                                                            let _ = crate::db::papers::update_citation_key(db.conn(), paper_id, &new_key).await;
                                                            lib_state.with_mut(|s| {
                                                                if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
                                                                    p.citation_key = Some(new_key);
                                                                }
                                                            });
                                                            editing_key.set(false);
                                                        });
                                                    }
                                                } else if evt.key() == Key::Escape {
                                                    editing_key.set(false);
                                                }
                                            }
                                        },
                                        onfocusout: move |_| {
                                            let new_key = edit_key_value().trim().to_string();
                                            if !new_key.is_empty() {
                                                let db = db_key2.clone();
                                                spawn(async move {
                                                    let _ = crate::db::papers::update_citation_key(db.conn(), paper_id, &new_key).await;
                                                    lib_state.with_mut(|s| {
                                                        if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
                                                            p.citation_key = Some(new_key);
                                                        }
                                                    });
                                                    editing_key.set(false);
                                                });
                                            } else {
                                                editing_key.set(false);
                                            }
                                        },
                                    }
                                }
                            } else {
                                div {
                                    class: "detail-value detail-value--cite-key",
                                    onclick: move |_| {
                                        if !copied_hint() {
                                            edit_key_value.set(key_display.clone());
                                            editing_key.set(true);
                                        }
                                    },
                                    if copied_hint() {
                                        code { class: "cite-key-copied-code", "Copied!" }
                                    } else {
                                        code { "{key_for_copy}" }
                                        button {
                                            class: "btn--ghost-sm cite-key-copy",
                                            title: "Copy citation key",
                                            onclick: move |evt| {
                                                evt.stop_propagation();
                                                let js = format!("navigator.clipboard.writeText({})", serde_json::to_string(&key_for_copy2).unwrap_or_default());
                                                let _ = document::eval(&js);
                                                copied_hint.set(true);
                                                spawn(async move {
                                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                                    copied_hint.set(false);
                                                });
                                            },
                                            i { class: "bi bi-clipboard" }
                                        }
                                    }
                                }
                            }
                        }
                    }
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

            // Notes section
            NotesSection { paper_id }

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
                    // Find Open Access PDF (when DOI exists but no PDF)
                    if paper.pdf_path.is_none() && paper.doi.is_some() {
                        {
                            let doi_for_oa = paper.doi.clone().unwrap_or_default();
                            let paper_title = paper.title.clone();
                            let paper_authors = paper.authors.clone();
                            let paper_year = paper.year;
                            let db_oa = db.clone();
                            let mut oa_status = use_signal(|| None::<String>);
                            rsx! {
                                button {
                                    class: "btn btn--secondary",
                                    disabled: oa_status().is_some(),
                                    onclick: move |_| {
                                        let db = db_oa.clone();
                                        let doi = doi_for_oa.clone();
                                        let title = paper_title.clone();
                                        let authors = paper_authors.clone();
                                        let year = paper_year;
                                        oa_status.set(Some("Searching...".to_string()));
                                        spawn(async move {
                                            match crate::metadata::unpaywall::fetch_oa_url(&doi).await {
                                                Ok(Some(pdf_url)) => {
                                                    oa_status.set(Some("Downloading...".to_string()));
                                                    // Download the PDF
                                                    match reqwest::get(&pdf_url).await {
                                                        Ok(resp) if resp.status().is_success() => {
                                                            match resp.bytes().await {
                                                                Ok(bytes) => {
                                                                    // Save to library
                                                                    let first_author = authors.first().map(|a| a.as_str());
                                                                    match db.import_pdf_bytes(&bytes, &title, first_author, year) {
                                                                        Ok(rel_path) => {
                                                                            let _ = crate::db::papers::update_pdf_path(db.conn(), paper_id, &rel_path).await;
                                                                            lib_state.with_mut(|s| {
                                                                                if let Some(p) = s.papers.iter_mut().find(|p| p.id == Some(paper_id)) {
                                                                                    p.pdf_path = Some(rel_path);
                                                                                }
                                                                            });
                                                                            oa_status.set(Some("PDF downloaded!".to_string()));
                                                                        }
                                                                        Err(e) => oa_status.set(Some(format!("Save failed: {e}"))),
                                                                    }
                                                                }
                                                                Err(e) => oa_status.set(Some(format!("Download failed: {e}"))),
                                                            }
                                                        }
                                                        Ok(resp) => oa_status.set(Some(format!("HTTP {}", resp.status()))),
                                                        Err(e) => oa_status.set(Some(format!("Request failed: {e}"))),
                                                    }
                                                }
                                                Ok(None) => oa_status.set(Some("No OA version found".to_string())),
                                                Err(e) => oa_status.set(Some(format!("Error: {e}"))),
                                            }
                                        });
                                    },
                                    if let Some(ref status) = oa_status() {
                                        "{status}"
                                    } else {
                                        "Find Open Access PDF"
                                    }
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
    let mut new_tag = use_signal(String::new);

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

#[component]
fn NotesSection(paper_id: i64) -> Element {
    let db = use_context::<Database>();
    let mut notes = use_signal(Vec::new);

    // Load notes for this paper
    {
        let db = db.clone();
        use_effect(move || {
            let db = db.clone();
            spawn(async move {
                if let Ok(paper_notes) =
                    crate::db::notes::list_notes_for_paper(db.conn(), paper_id).await
                {
                    notes.set(paper_notes);
                }
            });
        });
    }

    let note_list = notes.read();
    if note_list.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "detail-notes-section",
            label { class: "detail-label", "Notes ({note_list.len()})" }
            for note in note_list.iter() {
                {
                    let note_id = note.id.unwrap_or(0);
                    let title = note.title.clone();
                    let body_preview = if note.body.len() > 120 {
                        format!("{}...", &note.body[..117])
                    } else {
                        note.body.clone()
                    };
                    let db_del = db.clone();
                    rsx! {
                        div { key: "note-{note_id}", class: "detail-note-card",
                            div { class: "detail-note-title", "{title}" }
                            div { class: "detail-note-body", "{body_preview}" }
                            button {
                                class: "btn--danger-sm",
                                onclick: move |_| {
                                    let db = db_del.clone();
                                    spawn(async move {
                                        let _ = crate::db::notes::delete_note(db.conn(), note_id).await;
                                        if let Ok(paper_notes) = crate::db::notes::list_notes_for_paper(db.conn(), paper_id).await {
                                            notes.set(paper_notes);
                                        }
                                    });
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

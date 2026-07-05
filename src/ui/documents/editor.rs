//! Split-pane document editor: Typst/Markdown source left, compiled PDF preview right.

use dioxus::prelude::*;
use rotero_db::Database;
use rotero_models::{Document, DocumentFormat};

use super::code_editor::{CodeEditor, EditorLanguage};
use super::preview::DocumentPreview;
use crate::state::app_state::{LibraryState, LibraryView};

/// Bootstrap templates offered in the picker (a curated subset; the full
/// Universe list is fetched lazily in a later iteration).
const TEMPLATES: &[(&str, &str)] = &[("article", "Article (default)"), ("arkheion", "Preprint (arkheion)")];

/// CSL styles offered in the picker (subset of what Typst supports).
const STYLES: &[&str] = &["apa", "ieee", "vancouver", "nature", "mla", "chicago-author-date"];

#[component]
pub fn DocumentEditorPanel(document_id: String) -> Element {
    let db = use_context::<Database>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut doc_tabs = use_context::<Signal<crate::state::app_state::DocumentTabManager>>();

    // The working copy of the document, loaded once and edited in place.
    let mut doc = use_signal(|| None::<Document>);
    // Preview reload token: bumped after each successful compile.
    let mut preview_token = use_signal(|| 0u64);
    // Compile status message.
    let mut status = use_signal(String::new);
    // Out-of-band body to force into the CodeMirror editor (agent authoring).
    // The editor seeds from `value` on mount; later external rewrites arrive here.
    let mut external_body = use_signal(|| None::<String>);

    // Fetch Universe paper templates once (best-effort; the curated list works
    // offline). Merged into the template picker when the fetch succeeds.
    let universe_templates = use_resource(|| async move {
        tokio::task::spawn_blocking(rotero_typeset::list_templates)
            .await
            .ok()
            .and_then(|r| r.ok())
            .map(|list| {
                list.into_iter()
                    .filter(|t| t.categories.iter().any(|c| c == "paper"))
                    .map(|t| (format!("{}:{}", t.name, t.version), t.name))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });

    // Initial load of the document body.
    {
        let db = db.clone();
        let id = document_id.clone();
        use_effect(move || {
            let db = db.clone();
            let id = id.clone();
            spawn(async move {
                if let Ok(Some(d)) = rotero_db::documents::get_document(db.conn(), &id).await {
                    let token = d.last_pdf_path.is_some() as u64;
                    doc.set(Some(d));
                    preview_token.set(token);
                }
            });
        });
    }

    // Live agent authoring: when an MCP write fires the connector watch, re-read
    // this document's body so the editor and preview update in place. Reuses the
    // same signal the paper-list loader consumes (payload is `()` = "changed").
    #[cfg(feature = "desktop")]
    {
        let db = db.clone();
        let id = document_id.clone();
        use_future(move || {
            let db = db.clone();
            let id = id.clone();
            async move {
                let mut rx = {
                    let Some(lock) = crate::CONNECTOR_NOTIFY.get() else {
                        return;
                    };
                    let guard = lock.lock().unwrap();
                    guard.clone()
                };
                loop {
                    if rx.changed().await.is_err() {
                        break;
                    }
                    if let Ok(Some(fresh)) =
                        rotero_db::documents::get_document(db.conn(), &id).await
                    {
                        let recompiled = fresh.last_pdf_path.is_some();
                        // Push the agent's rewritten body into the live editor.
                        external_body.set(Some(fresh.body.clone()));
                        doc.set(Some(fresh));
                        if recompiled {
                            preview_token.with_mut(|t| *t += 1);
                        }
                    }
                }
            }
        });
    }

    let Some(current) = doc() else {
        return rsx! { div { class: "document-editor-loading", "Loading…" } };
    };

    // Debounced auto-save of a mutated document.
    let save = {
        let db = db.clone();
        move |updated: Document| {
            let db = db.clone();
            spawn(async move {
                let _ = rotero_db::documents::update_document(db.conn(), &updated).await;
            });
        }
    };

    let pdf_path = current.last_pdf_path.clone();

    // --- handlers ---
    // The CodeMirror editor reports the full document text on each edit.
    let on_body = {
        let save = save.clone();
        move |text: String| {
            let mut updated = doc().unwrap();
            if updated.body == text {
                return;
            }
            updated.body = text;
            doc.set(Some(updated.clone()));
            save(updated);
        }
    };
    let on_format = {
        let save = save.clone();
        move |e: FormEvent| {
            let mut updated = doc().unwrap();
            updated.format = DocumentFormat::from_str_or_default(&e.value());
            doc.set(Some(updated.clone()));
            save(updated);
        }
    };
    let on_title = {
        let save = save.clone();
        let id = document_id.clone();
        move |e: FormEvent| {
            let mut updated = doc().unwrap();
            updated.title = e.value();
            // Keep the open tab's label in sync.
            doc_tabs.with_mut(|m| m.set_title(&id, updated.title.clone()));
            doc.set(Some(updated.clone()));
            save(updated);
        }
    };
    let on_template = {
        let save = save.clone();
        move |e: FormEvent| {
            let mut updated = doc().unwrap();
            updated.template = e.value();
            doc.set(Some(updated.clone()));
            save(updated);
        }
    };
    let on_style = {
        let save = save.clone();
        move |e: FormEvent| {
            let mut updated = doc().unwrap();
            updated.csl_style = e.value();
            doc.set(Some(updated.clone()));
            save(updated);
        }
    };
    let on_collection = {
        let save = save.clone();
        move |e: FormEvent| {
            let mut updated = doc().unwrap();
            let v = e.value();
            updated.collection_id = if v.is_empty() { None } else { Some(v) };
            doc.set(Some(updated.clone()));
            save(updated);
        }
    };

    let compile = {
        let db = db.clone();
        move |_| {
            let db = db.clone();
            status.set("Compiling…".to_string());
            spawn(async move {
                let Some(mut d) = doc() else { return };

                // Build the bibliography from the linked collection.
                let bib = match &d.collection_id {
                    Some(cid) => build_bib(&db, cid).await,
                    None => None,
                };
                let opts = rotero_typeset::CompileOptions {
                    format: match d.format {
                        DocumentFormat::Markdown => rotero_typeset::DocumentFormat::Markdown,
                        DocumentFormat::Typst => rotero_typeset::DocumentFormat::Typst,
                    },
                    title: d.title.clone(),
                    authors: Vec::new(),
                    template: d.template.clone(),
                    bib,
                    csl_style: d.csl_style.clone(),
                };
                let body = d.body.clone();
                let result =
                    tokio::task::spawn_blocking(move || rotero_typeset::compile(&body, &opts))
                        .await;

                match result {
                    Ok(Ok(pdf)) => {
                        let dir = db.data_dir().join("documents");
                        let _ = std::fs::create_dir_all(&dir);
                        let id = d.id.clone().unwrap_or_default();
                        let path = dir.join(format!("{id}.pdf"));
                        if std::fs::write(&path, &pdf).is_ok() {
                            d.last_pdf_path = Some(path.to_string_lossy().to_string());
                            let _ = rotero_db::documents::update_document(db.conn(), &d).await;
                            doc.set(Some(d));
                            preview_token.with_mut(|t| *t += 1);
                            status.set(String::new());
                        } else {
                            status.set("Failed to write PDF".to_string());
                        }
                    }
                    Ok(Err(e)) => status.set(format!("Compile error: {e}")),
                    Err(e) => status.set(format!("Compile task failed: {e}")),
                }
            });
        }
    };

    let collections = lib_state.read().collections.clone();
    let cur_template = current.template.clone();
    let cur_style = current.csl_style.clone();
    let cur_collection = current.collection_id.clone().unwrap_or_default();
    let cur_format = current.format;
    let editor_language = match cur_format {
        DocumentFormat::Markdown => EditorLanguage::Markdown,
        DocumentFormat::Typst => EditorLanguage::Typst,
    };
    let editor_seed = current.body.clone();

    rsx! {
        div { class: "document-editor",
            div { class: "document-editor-toolbar",
                button {
                    class: "document-editor-back",
                    onclick: move |_| lib_state.with_mut(|s| s.view = LibraryView::Documents),
                    i { class: "bi bi-arrow-left" }
                }
                input {
                    class: "document-editor-title",
                    value: "{current.title}",
                    oninput: on_title,
                }
                select {
                    class: "select document-select",
                    title: "Collection this document draws citations from",
                    onchange: on_collection,
                    option { value: "", selected: cur_collection.is_empty(), "No collection" }
                    for c in collections.iter() {
                        {
                            let cid = c.id.clone().unwrap_or_default();
                            let sel = cid == cur_collection;
                            rsx! { option { value: "{cid}", selected: sel, "{c.name}" } }
                        }
                    }
                }
                select {
                    class: "select document-select",
                    title: "Template — sets the document type and layout",
                    onchange: on_template,
                    for (val, label) in TEMPLATES.iter() {
                        option { value: "{val}", selected: *val == cur_template, "{label}" }
                    }
                    if let Some(list) = &*universe_templates.read_unchecked() {
                        if !list.is_empty() {
                            optgroup { label: "Typst Universe",
                                for (val, label) in list.iter() {
                                    option {
                                        value: "{val}",
                                        selected: *val == cur_template,
                                        "{label}"
                                    }
                                }
                            }
                        }
                    }
                }
                select {
                    class: "select document-select",
                    title: "Citation style",
                    onchange: on_style,
                    for s in STYLES.iter() {
                        option { value: "{s}", selected: *s == cur_style, "{s}" }
                    }
                }
                select {
                    class: "select document-select",
                    title: "Source language — Typst for full paper control, Markdown for quick prose",
                    onchange: on_format,
                    option {
                        value: "typst",
                        selected: cur_format == DocumentFormat::Typst,
                        "Typst"
                    }
                    option {
                        value: "markdown",
                        selected: cur_format == DocumentFormat::Markdown,
                        "Markdown"
                    }
                }
                button { class: "document-editor-compile", onclick: compile,
                    i { class: "bi bi-play-fill" }
                    " Compile"
                }
                if !status().is_empty() {
                    span { class: "document-editor-status", "{status()}" }
                }
            }
            div { class: "document-editor-split",
                div { class: "document-editor-body",
                    CodeEditor {
                        value: editor_seed,
                        language: editor_language,
                        external_doc: external_body(),
                        on_change: on_body,
                    }
                }
                crate::ui::chat_panel::ResizeHandle {
                    target: "editor-split",
                    dir: crate::ui::chat_panel::ResizeDir::GrowRight,
                    min: 320.0,
                    max: 1100.0,
                    selector: ".document-editor-body".to_string(),
                }
                DocumentPreview { pdf_path, reload: preview_token() }
            }
        }
    }
}

/// Build a BibTeX string from a collection's papers, or `None` if empty/missing.
async fn build_bib(db: &Database, collection_id: &str) -> Option<String> {
    let ids = rotero_db::collections::list_paper_ids_in_subtree(db.conn(), collection_id)
        .await
        .ok()?;
    if ids.is_empty() {
        return None;
    }
    let papers = rotero_db::papers::get_papers_by_ids(db.conn(), &ids)
        .await
        .ok()?;
    if papers.is_empty() {
        None
    } else {
        Some(rotero_bib::export_bibtex(&papers))
    }
}

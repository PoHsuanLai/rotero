//! The Documents list: shows all authored documents and creates new ones.

use dioxus::prelude::*;
use rotero_db::Database;
use rotero_models::{Document, DocumentKind};

use crate::state::app_state::{DocumentTabManager, LibraryState, LibraryView};

#[component]
pub fn DocumentsListPanel() -> Element {
    let db = use_context::<Database>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut doc_tabs = use_context::<Signal<DocumentTabManager>>();

    // A local generation counter to force reloads after create/delete.
    let mut generation = use_signal(|| 0u64);

    // Load the document list, re-running whenever `generation` changes.
    let docs = use_resource({
        let db = db.clone();
        move || {
            let db = db.clone();
            let _gen = generation();
            async move {
                rotero_db::documents::list_documents(db.conn())
                    .await
                    .unwrap_or_default()
            }
        }
    });

    let collections = lib_state.read().collections.clone();

    // Local search query, filtering the document list by title (client-side).
    // Kept separate from the library's paper-search state.
    let mut query = use_signal(String::new);

    // One "New Document". The document type is expressed by its Typst template
    // (chosen in the editor), not a fixed kind; `kind` defaults to a neutral
    // value here and is only set meaningfully by the agent.
    let create_doc = move |_| {
        let db = db.clone();
        spawn(async move {
            let title = "Untitled Document".to_string();
            let mut doc = Document::new(title.clone(), DocumentKind::Summary, None);
            // Seed a minimal Typst skeleton so the document compiles immediately
            // and shows the author what the source language looks like.
            doc.body = "= Untitled Document\n\n\
                Start writing in Typst. Use `= Heading` for sections, `$x^2$` for \
                math, and `@citekey` to cite papers from a linked collection.\n"
                .to_string();
            if let Ok(id) = rotero_db::documents::insert_document(db.conn(), &doc).await {
                generation.with_mut(|g| *g += 1);
                doc_tabs.with_mut(|m| {
                    m.open_or_switch(id.clone(), title);
                });
                lib_state.with_mut(|s| s.view = LibraryView::Document(id));
            }
        });
    };

    // Apply the title filter and compute the visible count.
    let all_docs = docs.read_unchecked().clone();
    let q = query().trim().to_lowercase();
    let filtered: Option<Vec<Document>> = all_docs.as_ref().map(|list| {
        list.iter()
            .filter(|d| q.is_empty() || d.title.to_lowercase().contains(&q))
            .cloned()
            .collect()
    });
    let count = filtered.as_ref().map(|f| f.len()).unwrap_or(0);

    rsx! {
        div { class: "library-view",
            div { class: "library-header",
                div { class: "library-header-left",
                    h2 { class: "library-title", "Documents" }
                    span { class: "library-count", "{count} documents" }
                }
                div { class: "library-header-right",
                    crate::ui::chat_panel::ChatToggleButton {}
                    button { class: "btn btn--primary btn--sm", onclick: create_doc,
                        i { class: "bi bi-plus-lg" }
                        span { " New Document" }
                    }
                }
            }

            div { class: "search-sort-row",
                div { class: "search-bar",
                    i { class: "search-icon bi bi-search" }
                    input {
                        class: "input input--lg search-input",
                        r#type: "text",
                        placeholder: "Search documents...",
                        value: "{query}",
                        oninput: move |evt| query.set(evt.value()),
                    }
                    if !query().is_empty() {
                        button {
                            class: "search-clear",
                            onclick: move |_| query.set(String::new()),
                            i { class: "bi bi-x-lg" }
                        }
                    }
                }
            }

            div { class: "library-list",
                match &filtered {
                    Some(list) if !list.is_empty() => rsx! {
                        for doc in list.clone() {
                            DocumentCard { doc, collections: collections.clone() }
                        }
                    },
                    Some(_) if !q.is_empty() => rsx! {
                        div { class: "documents-empty", p { "No documents match your search." } }
                    },
                    Some(_) => rsx! {
                        div { class: "documents-empty",
                            p { "No documents yet." }
                            p { class: "documents-empty-hint",
                                "Create one above, or ask the assistant to summarize a collection."
                            }
                        }
                    },
                    None => rsx! { div { class: "documents-empty", "Loading…" } },
                }
            }
        }
    }
}

#[component]
fn DocumentCard(doc: Document, collections: Vec<rotero_models::Collection>) -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let mut doc_tabs = use_context::<Signal<DocumentTabManager>>();
    let id = doc.id.clone().unwrap_or_default();
    let title = doc.title.clone();

    let coll_name = doc.collection_id.as_ref().and_then(|cid| {
        collections
            .iter()
            .find(|c| c.id.as_deref() == Some(cid.as_str()))
            .map(|c| c.name.clone())
    });

    // The "type" shown is the template (the thing that actually shapes output).
    let template_label = template_display(&doc.template);

    let open = move |_| {
        let id = id.clone();
        let title = title.clone();
        doc_tabs.with_mut(|m| {
            m.open_or_switch(id.clone(), title);
        });
        lib_state.with_mut(|s| s.view = LibraryView::Document(id));
    };

    // Reuse the library paper-card layout so documents read as first-class
    // library objects. Meta line: template · collection · compiled state.
    rsx! {
        div { class: "library-card", onclick: open,
            div { class: "library-card-indicator" }

            div { class: "library-card-body",
                div { class: "library-card-title", "{doc.title}" }
                div { class: "library-card-meta",
                    span { class: "library-card-authors", "{template_label}" }
                    if let Some(name) = coll_name {
                        span { class: "library-card-sep", "\u{00b7}" }
                        span { class: "library-card-journal", "{name}" }
                    }
                    if doc.last_pdf_path.is_some() {
                        span { class: "library-card-sep", "\u{00b7}" }
                        span { class: "library-card-citations", "compiled" }
                    }
                }
            }
        }
    }
}

/// Human-friendly label for a template identifier ("name" or "name:version").
fn template_display(template: &str) -> String {
    let name = template.split(':').next().unwrap_or(template);
    match name {
        "article" | "" => "Article".to_string(),
        "arkheion" => "Preprint".to_string(),
        other => other.to_string(),
    }
}

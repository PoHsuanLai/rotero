use dioxus::prelude::*;

use crate::ui::chat_panel::ResizeHandle;
use crate::ui::components::context_menu::{ContextMenu, ContextMenuItem};
use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use crate::sync::engine::SyncConfig;
use rotero_db::Database;

use super::fields::{AddToCollectionSelect, TagEditor};
use super::notes::NotesSection;

#[component]
pub fn PaperDetail() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();

    let state = lib_state.read();
    let paper = match state.selected_paper() {
        Some(p) => p.clone(),
        None => return rsx! {},
    };
    drop(state);

    let paper_id = paper.id.clone().unwrap_or_default();
    let pid_oa = paper_id.clone();
    let pid_del = paper_id.clone();
    let pid_open = paper_id.clone();
    let authors_display = if paper.authors.is_empty() {
        "Unknown".to_string()
    } else {
        paper.authors.join(", ")
    };

    // DOI context menu state: (doi_string, x, y)
    let mut doi_ctx = use_signal(|| None::<(String, f64, f64)>);

    // Hooks for citation key editing (must be unconditional)
    let mut editing_key = use_signal(|| false);
    let mut edit_key_value = use_signal(|| paper.citation.citation_key.clone().unwrap_or_default());
    let mut copied_hint = use_signal(|| false);

    // Hook for Open Access PDF download status (must be unconditional)
    let mut oa_status = use_signal(|| None::<String>);

    // Reset OA status when selected paper changes
    let _ = use_memo(move || {
        let _ = lib_state.read().selected_paper_id.clone();
        oa_status.set(None);
    });

    rsx! {
        div { class: "paper-detail",
            ResizeHandle { target: "detail" }

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
            if let Some(count) = paper.citation.citation_count {
                div { class: "detail-field",
                    label { class: "detail-label", "Citations" }
                    div { class: "detail-value detail-value--citations", "{count}" }
                }
            }

            // Citation key
            if let Some(ref cite_key) = paper.citation.citation_key {
                {
                    let key_for_copy = cite_key.clone();
                    let key_for_copy2 = cite_key.clone();
                    let key_display = cite_key.clone();
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
                                            let paper_id = paper_id.clone();
                                            move |evt: Event<KeyboardData>| {
                                                if evt.key() == Key::Enter {
                                                    let new_key = edit_key_value().trim().to_string();
                                                    if !new_key.is_empty() {
                                                        let db = db.clone();
                                                        let pid = paper_id.clone();
                                                        spawn(async move {
                                                            let _ = rotero_db::papers::update_citation_key(db.conn(), &pid, &new_key).await;
                                                            let pid2 = pid.clone();
                                                            lib_state.with_mut(|s| {
                                                                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                                                    p.citation.citation_key = Some(new_key);
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
                                        onfocusout: {
                                            let paper_id = paper_id.clone();
                                            move |_| {
                                            let new_key = edit_key_value().trim().to_string();
                                            if !new_key.is_empty() {
                                                let db = db_key2.clone();
                                                let pid = paper_id.clone();
                                                spawn(async move {
                                                    let _ = rotero_db::papers::update_citation_key(db.conn(), &pid, &new_key).await;
                                                    let pid2 = pid.clone();
                                                    lib_state.with_mut(|s| {
                                                        if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                                            p.citation.citation_key = Some(new_key);
                                                        }
                                                    });
                                                    editing_key.set(false);
                                                });
                                            } else {
                                                editing_key.set(false);
                                            }
                                        }},
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
                                                if let Ok(mut clip) = arboard::Clipboard::new() {
                                                    let _ = clip.set_text(&*key_for_copy2);
                                                }
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
            if let Some(ref journal) = paper.publication.journal {
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
                AddToCollectionSelect { paper_id: paper_id.clone() }
            }

            // Tags
            div { class: "detail-field",
                label { class: "detail-label", "Tags" }
                TagEditor { paper_id: paper_id.clone() }
            }

            // Citation button
            div { class: "detail-cite-section",
                crate::ui::citation_dialog::CitationDialog {}
            }

            // Notes section
            NotesSection { paper_id: paper_id.clone() }

            // Open / Delete buttons
            div { class: "detail-delete-section",
                div { class: "detail-actions",
                    if paper.links.pdf_path.is_some() {
                        {
                            let pdf_rel_path = paper.links.pdf_path.clone();
                            let title = paper.title.clone();
                            let db_open = db.clone();
                            rsx! {
                                button {
                                    class: "btn btn--primary",
                                    onclick: move |_| {
                                        if let Some(ref rel_path) = pdf_rel_path {
                                            let full_path = db_open.resolve_pdf_path(rel_path);
                                            let path_str = full_path.to_string_lossy().to_string();
                                            let cfg = config.read();
                                            tabs.with_mut(|m| m.open_or_switch(pid_open.clone(), path_str, title.clone(), cfg.pdf.default_zoom, cfg.pdf.page_batch_size, dpr_sig.read().0));
                                            lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                                        }
                                    },
                                    "Open Paper"
                                }
                            }
                        }
                    }
                    // Find Open Access PDF (when DOI exists but no PDF)
                    if paper.links.pdf_path.is_none() && paper.doi.is_some() {
                        {
                            let doi_for_oa = paper.doi.clone().unwrap_or_default();
                            let paper_title = paper.title.clone();
                            let paper_authors = paper.authors.clone();
                            let paper_year = paper.year;
                            let db_oa = db.clone();
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
                                        let paper_id = pid_oa.clone();
                                        oa_status.set(Some("Searching...".to_string()));
                                        spawn(async move {
                                            // Try arXiv first if DOI looks like an arXiv DOI
                                            let arxiv_id = doi
                                                .strip_prefix("10.48550/arXiv.")
                                                .or_else(|| doi.strip_prefix("arXiv:"))
                                                .map(|s| s.to_string());

                                            let pdf_url = if let Some(ref aid) = arxiv_id {
                                                // arXiv PDF is always available
                                                Ok(Some(format!("https://arxiv.org/pdf/{aid}")))
                                            } else {
                                                crate::metadata::unpaywall::fetch_oa_url(&doi).await
                                            };

                                            match pdf_url {
                                                Ok(Some(pdf_url)) => {
                                                    oa_status.set(Some("Downloading...".to_string()));
                                                    match reqwest::get(&pdf_url).await {
                                                        Ok(resp) if resp.status().is_success() => {
                                                            match resp.bytes().await {
                                                                Ok(bytes) => {
                                                                    let first_author = authors.first().map(|a| a.as_str());
                                                                    match db.import_pdf_bytes(&bytes, &title, first_author, year) {
                                                                        Ok(rel_path) => {
                                                                            let pid = paper_id.clone();
                                                                            let _ = rotero_db::papers::update_pdf_path(db.conn(), &pid, &rel_path).await;
                                                                            let pid2 = pid.clone();
                                                                            lib_state.with_mut(|s| {
                                                                                if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                                                                    p.links.pdf_path = Some(rel_path);
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
                            let pid = pid_del.clone();
                            move |_| {
                                let db = db_del.clone();
                                let pid = pid.clone();
                                spawn(async move {
                                    if let Ok(()) = rotero_db::papers::delete_paper(db.conn(), &pid).await {
                                        let pid2 = pid.clone();
                                        lib_state.with_mut(|s| {
                                            s.papers.retain(|p| p.id.as_deref() != Some(pid2.as_str()));
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
                                    if let Ok(mut clip) = arboard::Clipboard::new() {
                                        let _ = clip.set_text(&*doi_copy);
                                    }
                                    doi_ctx.set(None);
                                },
                            }

                            ContextMenuItem {
                                label: "Open in browser".to_string(),
                                icon: Some("bi-box-arrow-up-right".to_string()),
                                on_click: move |_| {
                                    let url = format!("https://doi.org/{}", doi_open);
                                    let _ = open::that(&url);
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

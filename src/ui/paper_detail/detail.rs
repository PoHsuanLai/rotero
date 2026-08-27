use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, PdfTabManager};
use crate::sync::engine::SyncConfig;
use rotero_db::Database;

use super::DetailShell;
use super::conversations::ConversationsSection;
use super::detail_fields::DetailFields;
use super::fields::{CollectionsField, TagsField};
use super::notes::NotesSection;
use super::title_field::EditableTitleField;

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

    // A Rename raised from the library context menu, for this paper.
    let rename_requested = lib_state
        .read()
        .rename_paper_id
        .as_deref()
        .is_some_and(|id| id == paper_id);

    let mut editing_key = use_signal(|| false);
    let mut edit_key_value = use_signal(|| paper.citation.citation_key.clone().unwrap_or_default());
    let mut copied_hint = use_signal(|| false);

    let mut oa_statuses = use_context::<Signal<std::collections::HashMap<String, String>>>();
    let oa_status_value = oa_statuses.read().get(&paper_id).cloned();

    rsx! {
        DetailShell {
            div { class: "detail-header",
                h3 { class: "detail-heading", "Details" }
                button {
                    class: "detail-close",
                    onclick: move |_| {
                        lib_state.with_mut(|s| s.clear_selection());
                    },
                    "\u{00d7}"
                }
            }

            EditableTitleField {
                paper_id: paper_id.clone(),
                title: paper.title.clone(),
                item_type: paper.item_type.clone(),
                external_trigger: rename_requested,
            }

            DetailFields { paper: paper.clone() }

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
                                        onfocusin: crate::ui::keybindings::editable_focus_in,
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
                                                            let _ = db.update_citation_key(&pid, &new_key).await;
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
                                            move |evt| {
                                            crate::ui::keybindings::editable_focus_out(evt);
                                            let new_key = edit_key_value().trim().to_string();
                                            if !new_key.is_empty() {
                                                let db = db_key2.clone();
                                                let pid = paper_id.clone();
                                                spawn(async move {
                                                    let _ = db.update_citation_key(&pid, &new_key).await;
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

            div { class: "detail-field",
                label { class: "detail-label", "Collection" }
                CollectionsField { key: "{paper_id}", paper_id: paper_id.clone() }
            }

            div { class: "detail-field",
                label { class: "detail-label", "Tags" }
                TagsField { key: "{paper_id}", paper_id: paper_id.clone() }
            }

            div { class: "detail-cite-section",
                crate::ui::citation_dialog::CitationDialog {}
            }

            ConversationsSection { paper_id: paper_id.clone() }
            NotesSection { paper_id: paper_id.clone() }

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
                                            crate::state::commands::open_paper_pdf(&db_open, &mut tabs, &mut lib_state, &config, &dpr_sig, &pid_open, rel_path, &title);
                                        }
                                    },
                                    "Open Paper"
                                }
                            }
                        }
                    }
                    if paper.links.pdf_path.is_none() {
                        {
                            let doi_for_oa = paper.doi.clone();
                            let url_for_oa = paper.links.pdf_url.clone();
                            let paper_title = paper.title.clone();
                            let paper_authors = paper.author_names();
                            let paper_year = paper.year;
                            let db_oa = db.clone();
                            let agent_title = paper.title.clone();
                            let agent_doi = paper.doi.clone();
                            let agent_authors = paper.author_names();
                            let agent_year = paper.year;
                            let agent_pid = pid_oa.clone();
                            let agent_channel = use_context::<crate::ui::chat_panel::AgentChannel>();
                            let mut chat_state = use_context::<Signal<crate::agent::types::ChatState>>();
                            let is_ask_agent = oa_status_value.as_deref() == Some("ask_agent");
                            let is_busy = oa_status_value.is_some() && !is_ask_agent;
                            rsx! {
                                button {
                                    class: if is_ask_agent { "btn btn--primary" } else { "btn btn--secondary" },
                                    disabled: is_busy,
                                    onclick: move |_| {
                                        if is_ask_agent {
                                            // Delegate to agent
                                            let title = agent_title.clone();
                                            let doi = agent_doi.clone();
                                            let authors = agent_authors.clone();
                                            let year = agent_year;
                                            let pid = agent_pid.clone();
                                            let prompt = format!(
                                                "Find and download the open access PDF for this paper:\n\
                                                 Title: {title}\n\
                                                 Authors: {}\n\
                                                 Year: {}\n\
                                                 DOI: {}\n\
                                                 Paper ID: {pid}\n\n\
                                                 The automated OA search couldn't find it. Please search the web for a freely \
                                                 available PDF of this paper (check the conference website, author pages, \
                                                 institutional repositories, etc.) and use the download_pdf tool to save it.",
                                                authors.join(", "),
                                                year.map(|y| y.to_string()).unwrap_or_default(),
                                                doi.as_deref().unwrap_or("none"),
                                            );
                                            let paper_context = Some(format!(
                                                "<rotero-context>\nPaper ID: {pid}\nTitle: {title}\n</rotero-context>"
                                            ));
                                            chat_state.with_mut(|s| {
                                                s.panel_open = true;
                                                s.messages.push(crate::agent::types::ChatMessage::hidden(
                                                    crate::agent::types::ChatRole::User,
                                                    vec![crate::agent::types::MessageContent::Text(prompt.clone())],
                                                ));
                                                s.status = crate::agent::types::AgentStatus::Streaming;
                                            });
                                            agent_channel.send(crate::agent::types::ChatRequest::SendMessage {
                                                prompt,
                                                paper_context,
                                            });
                                            oa_statuses.with_mut(|m| { m.insert(pid, "Agent...".into()); });
                                        } else {
                                            // Automated OA search
                                            let db = db_oa.clone();
                                            let doi = doi_for_oa.clone();
                                            let direct_url = url_for_oa.clone();
                                            let title = paper_title.clone();
                                            let authors = paper_authors.clone();
                                            let year = paper_year;
                                            let paper_id = pid_oa.clone();
                                            oa_statuses.with_mut(|m| { m.insert(paper_id.clone(), "Searching...".into()); });
                                            spawn(async move {
                                                let urls = crate::metadata::pdf_download::resolve_pdf_urls(direct_url.as_deref(), doi.as_deref(), &title).await;
                                                if urls.is_empty() {
                                                    oa_statuses.with_mut(|m| { m.insert(paper_id.clone(), "ask_agent".into()); });
                                                    return;
                                                }
                                                oa_statuses.with_mut(|m| { m.insert(paper_id.clone(), "Downloading...".into()); });
                                                let first_author = authors.first().map(|a| a.as_str());
                                                match crate::metadata::pdf_download::download_and_save_pdf(&db, &urls, &title, first_author, year).await {
                                                    Ok(rel_path) => {
                                                        let pid = paper_id.clone();
                                                        let _ = db.update_pdf_path(&pid, &rel_path).await;
                                                        let pid2 = pid.clone();
                                                        lib_state.with_mut(|s| {
                                                            if let Some(p) = s.papers.iter_mut().find(|p| p.id.as_deref() == Some(pid2.as_str())) {
                                                                p.links.pdf_path = Some(rel_path);
                                                            }
                                                        });
                                                        oa_statuses.with_mut(|m| { m.insert(paper_id, "Downloaded".into()); });
                                                    }
                                                    Err(_) => {
                                                        oa_statuses.with_mut(|m| { m.insert(paper_id, "ask_agent".into()); });
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    if is_ask_agent {
                                        "Ask Agent"
                                    } else if let Some(ref status) = oa_status_value {
                                        "{status}"
                                    } else {
                                        "Find PDF"
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
                                    if let Ok(()) = db.delete_paper(&pid).await {
                                        let pid2 = pid.clone();
                                        lib_state.with_mut(|s| {
                                            s.papers.retain(|p| p.id.as_deref() != Some(pid2.as_str()));
                                            s.clear_selection();
                                        });
                                    }
                                });
                            }
                        },
                        "Delete Paper"
                    }
                }
            }
        }
    }
}

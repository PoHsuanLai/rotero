//! The agent conversations a paper belongs to.
//!
//! Sits alongside notes because it is the same kind of thing: something you
//! wrote about this paper and will want back. A conversation covering several
//! papers appears under each of them, which is how a group chat is found again
//! without remembering the exact selection that started it.

use dioxus::prelude::*;
use rotero_db::Database;
use rotero_db::chat_sessions::{ChatSessionRow, ChatSubject};

use crate::agent::types::ChatState;
use crate::state::app_state::LibraryState;
use crate::ui::chat_panel::{AgentChannel, switch_to};

/// How a conversation's subject reads in this list.
fn describe(row: &ChatSessionRow, paper_id: &str, lib: &LibraryState) -> String {
    match row.subject_kind.as_str() {
        "collection" => row
            .subject_id
            .as_ref()
            .and_then(|id| {
                lib.collections
                    .iter()
                    .find(|c| c.id.as_deref() == Some(id.as_str()))
            })
            .map(|c| format!("About the collection “{}”", c.name))
            .unwrap_or_else(|| "About a collection".into()),
        "group" => "Part of a conversation about several papers".into(),
        // The common case needs no explanation: the conversation is about the
        // paper whose panel this is.
        _ if row.subject_id.as_deref() == Some(paper_id) => String::new(),
        _ => "About another paper".into(),
    }
}

/// The subject to resume a listed conversation under.
fn subject_of(row: &ChatSessionRow, papers: Vec<String>) -> ChatSubject {
    match (row.subject_kind.as_str(), row.subject_id.as_deref()) {
        ("collection", Some(id)) => ChatSubject::Collection(id.to_string()),
        ("group", _) => ChatSubject::Group(papers),
        (_, Some(id)) => ChatSubject::Paper(id.to_string()),
        _ => ChatSubject::Group(papers),
    }
}

#[component]
pub fn ConversationsSection(paper_id: String) -> Element {
    let db = use_context::<Database>();
    let lib_state = use_context::<Signal<LibraryState>>();
    let mut chat_state = use_context::<Signal<ChatState>>();
    let agent_channel = use_context::<AgentChannel>();
    // `use_reactive` is what ties this to the prop: the panel reuses this
    // component when the selection changes, and a plain captured clone is not
    // reactive, so the query would run once and every later paper would show
    // the first one's conversations.
    let db_load = db.clone();
    let loaded = use_resource(use_reactive!(|paper_id| {
        let db = db_load.clone();
        async move {
            let rows = db
                .chat_sessions_for_paper(&paper_id)
                .await
                .unwrap_or_default();
            let mut with_papers = Vec::new();
            for row in rows {
                let papers = db
                    .chat_session_paper_ids(&row.session_id)
                    .await
                    .unwrap_or_default();
                with_papers.push((row, papers));
            }
            with_papers
        }
    }));

    let listed = loaded.read();
    let listed = match listed.as_ref() {
        Some(rows) => rows,
        None => return rsx! {},
    };
    if listed.is_empty() {
        return rsx! {};
    }

    rsx! {
        div { class: "detail-chats-section",
            label { class: "detail-label", "Conversations ({listed.len()})" }
            for (row, papers) in listed.iter() {
                {
                    let summary = row.summary.clone().unwrap_or_default();
                    let context = describe(row, &paper_id, &lib_state.read());
                    let subject = subject_of(row, papers.clone());
                    let db_open = db.clone();
                    rsx! {
                        button {
                            key: "chat-{row.session_id}",
                            class: "detail-chat-card",
                            onclick: move |_| {
                                chat_state.with_mut(|s| s.panel_open = true);
                                switch_to(&mut chat_state, &agent_channel, &db_open, subject.clone());
                            },
                            if summary.is_empty() {
                                div { class: "detail-chat-summary detail-chat-summary--empty",
                                    "Untitled conversation"
                                }
                            } else {
                                div { class: "detail-chat-summary", "{summary}" }
                            }
                            if !context.is_empty() {
                                div { class: "detail-chat-context", "{context}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

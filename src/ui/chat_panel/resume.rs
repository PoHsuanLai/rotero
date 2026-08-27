//! Keeping the panel's conversation pointed at what the user is reading.
//!
//! A chat belongs to its subject, so opening a paper should continue that
//! paper's conversation rather than whatever was last on screen. Switching is
//! automatic only while the panel is idle: doing it mid-reply would abandon a
//! conversation the user is still waiting on, so that case asks first.

use dioxus::prelude::*;
use rotero_db::Database;
use rotero_db::chat_sessions::ChatSubject;

use crate::agent::types::{AgentStatus, ChatRequest, ChatState, PendingSwitch};
use crate::state::app_state::{LibraryState, PdfTabManager};

use super::{AgentChannel, current_subject};

/// How to name a subject in the UI.
pub fn subject_label(subject: &ChatSubject, lib: &LibraryState) -> String {
    match subject {
        ChatSubject::Paper(id) => lib
            .papers
            .iter()
            .find(|p| p.id.as_deref() == Some(id.as_str()))
            .map(|p| p.title.clone())
            .unwrap_or_else(|| "this paper".into()),
        ChatSubject::Collection(id) => lib
            .collections
            .iter()
            .find(|c| c.id.as_deref() == Some(id.as_str()))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "this collection".into()),
        ChatSubject::Group(ids) => format!("{} papers", ids.len()),
    }
}

/// Whether the panel can switch conversations without losing anything.
fn is_idle(state: &ChatState) -> bool {
    matches!(state.status, AgentStatus::Idle) && state.input_text.trim().is_empty()
}

/// Point the panel at `subject`, resuming its conversation or starting one.
pub fn switch_to(
    chat_state: &mut Signal<ChatState>,
    agent_channel: &AgentChannel,
    db: &Database,
    subject: ChatSubject,
) {
    chat_state.with_mut(|s| {
        s.messages.clear();
        s.current_subject = Some(subject.clone());
        s.pending_subject = Some(subject.clone());
        s.pending_switch = None;
        s.declined_subject = None;
        s.current_session_id = None;
    });

    let db = db.clone();
    let agent_channel = *agent_channel;
    let mut chat_state = *chat_state;
    spawn(async move {
        match db.chat_session_for_subject(&subject).await {
            Ok(Some(existing)) => {
                chat_state.with_mut(|s| s.status = AgentStatus::Connecting);
                agent_channel.send(ChatRequest::LoadSession {
                    session_id: existing.session_id,
                    cwd: String::new(),
                });
            }
            // Nothing to resume: the next message starts this subject's
            // conversation, and `pending_subject` already says what it is about.
            Ok(None) => {}
            Err(e) => tracing::debug!("chat: looking up a conversation failed: {e}"),
        }
    });
}

/// Follows the active subject, switching the panel's conversation to match.
#[component]
pub fn SubjectFollower() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let lib_state = use_context::<Signal<LibraryState>>();
    let tab_mgr = use_context::<Signal<PdfTabManager>>();
    let agent_channel = use_context::<AgentChannel>();
    let db = use_context::<Database>();

    use_effect(move || {
        // Read outside any spawn so the effect actually subscribes to these.
        let panel_open = chat_state.read().panel_open;
        let subject = current_subject(&lib_state.read(), &tab_mgr.read());

        if !panel_open {
            return;
        }
        let Some(subject) = subject else {
            // No subject — a general chat. Leave whatever is on screen alone
            // rather than clearing it for nothing.
            return;
        };
        if chat_state.read().current_subject.as_ref() == Some(&subject) {
            return;
        }
        // Asking twice about the same subject would nag on every render, and a
        // subject already declined should stay declined.
        if chat_state
            .read()
            .pending_switch
            .as_ref()
            .is_some_and(|p| p.subject == subject)
            || chat_state.read().declined_subject.as_ref() == Some(&subject)
        {
            return;
        }

        if is_idle(&chat_state.read()) {
            switch_to(&mut chat_state, &agent_channel, &db, subject);
        } else {
            let label = subject_label(&subject, &lib_state.read());
            chat_state.with_mut(|s| {
                s.pending_switch = Some(PendingSwitch { subject, label });
            });
        }
    });

    rsx! {}
}

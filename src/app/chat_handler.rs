use dioxus::prelude::*;
use rotero_db::Database;
use rotero_db::chat_sessions::ChatSessionRow;

use super::chat_papers::paper_ids_from_tool_output;
use crate::agent::types::{
    AgentStatus, ChatEvent, ChatMessage, ChatRole, ChatState, MessageContent,
};
use crate::state::app_state::LibraryState;

/// The agent thread's event receiver, handed to [`ChatEventPump`].
///
/// Held in a signal so the pump can take ownership once, on first render.
#[derive(Clone, Copy)]
pub struct AgentEvents {
    pub inner: Signal<Option<tokio::sync::mpsc::UnboundedReceiver<ChatEvent>>>,
}

/// Drains the agent thread's events into [`ChatState`].
///
/// A component rather than a bare future so it can reach the `Database`, which
/// only exists once the library is open: recording which papers a conversation
/// touched is part of handling those events.
#[component]
pub fn ChatEventPump() -> Element {
    let db = use_context::<Database>();
    let mut chat_state = use_context::<Signal<ChatState>>();
    let lib_state = use_context::<Signal<LibraryState>>();
    let events = use_context::<AgentEvents>();

    use_future(move || {
        let db = db.clone();
        let mut rx_sig = events.inner;
        async move {
            let Some(mut rx) = rx_sig.write().take() else {
                return;
            };
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                while let Ok(event) = rx.try_recv() {
                    handle_chat_event(&mut chat_state, &lib_state, &db, event);
                }
            }
        }
    });

    rsx! {}
}

/// Record papers a conversation touched, keeping only ids the library knows.
///
/// Best-effort: chat bookkeeping must never interrupt the stream, so failures
/// are logged and dropped rather than surfaced.
fn link_papers(
    chat_state: &Signal<ChatState>,
    lib_state: &Signal<LibraryState>,
    db: &Database,
    paper_ids: Vec<String>,
) {
    let Some(session_id) = chat_state.read().current_session_id.clone() else {
        return;
    };
    let known: Vec<String> = {
        let lib = lib_state.read();
        paper_ids
            .into_iter()
            .filter(|id| lib.papers.iter().any(|p| p.id.as_deref() == Some(id)))
            .collect()
    };
    if known.is_empty() {
        return;
    }
    let db = db.clone();
    spawn(async move {
        for paper_id in known {
            if let Err(e) = db.link_chat_session_paper(&session_id, &paper_id).await {
                tracing::debug!("chat: linking {paper_id} failed: {e}");
            }
        }
    });
}

pub fn handle_chat_event(
    chat_state: &mut Signal<ChatState>,
    lib_state: &Signal<LibraryState>,
    db: &Database,
    event: ChatEvent,
) {
    match event {
        ChatEvent::Switching { provider_id } => {
            chat_state.with_mut(|s| {
                s.messages.clear();
                s.commands.clear();
                s.session_active = false;
                s.auth_methods.clear();
                s.status = AgentStatus::Connecting;
                s.active_provider_id = provider_id;
            });
        }
        ChatEvent::Connected {
            auth_methods,
            provider_id,
            supports_list_sessions,
        } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Connecting;
                s.auth_methods = auth_methods;
                s.active_provider_id = provider_id;
                s.supports_list_sessions = supports_list_sessions;
            });
        }
        ChatEvent::SessionCreated { session_id } => {
            let subject = chat_state.read().pending_subject.clone();
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Idle;
                s.session_active = true;
                s.current_session_id = Some(session_id.clone());
            });

            // Without a subject the conversation is a general one; it is still
            // worth recording, so it can be found once a subject is inferred
            // from the papers it goes on to touch.
            let provider_id = chat_state.read().active_provider_id.clone();
            let now = chrono::Utc::now().to_rfc3339();
            let paper_ids = subject.as_ref().map(|s| s.paper_ids()).unwrap_or_default();
            let row = ChatSessionRow {
                session_id,
                provider_id,
                subject_kind: subject
                    .as_ref()
                    .map(|s| s.kind().to_string())
                    .unwrap_or_else(|| "general".into()),
                subject_id: subject.as_ref().map(|s| s.id()),
                summary: None,
                created_at: now.clone(),
                last_used_at: now,
                is_dead: false,
            };
            let db = db.clone();
            spawn(async move {
                if let Err(e) = db.upsert_chat_session(&row, &paper_ids).await {
                    tracing::debug!("chat: recording session failed: {e}");
                }
            });
        }
        ChatEvent::UserMessage {
            text,
            context_paper_ids,
        } => {
            // A replayed transcript is the only place an older conversation's
            // subject survives, so capture before rendering.
            link_papers(chat_state, lib_state, db, context_paper_ids);
            if !text.is_empty() {
                chat_state.with_mut(|s| {
                    s.messages.push(ChatMessage::new(
                        ChatRole::User,
                        vec![MessageContent::Text(text)],
                    ));
                });
            }
        }
        ChatEvent::TextDelta(text) => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Streaming;
                if let Some(last) = s.messages.last_mut()
                    && last.role == ChatRole::Assistant
                {
                    if let Some(MessageContent::Text(t)) = last.content.last_mut() {
                        t.push_str(&text);
                    } else {
                        last.content.push(MessageContent::Text(text));
                    }
                    return;
                }
                s.messages
                    .push(ChatMessage::assistant(vec![MessageContent::Text(text)]));
            });
        }
        ChatEvent::ToolCallStarted { id, title } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::ToolCall(title.clone());
                if s.messages.last().map(|m| &m.role) != Some(&ChatRole::Assistant) {
                    s.messages.push(ChatMessage::assistant(vec![]));
                }
                if let Some(last) = s.messages.last_mut() {
                    last.content.push(MessageContent::ToolUse {
                        id,
                        title,
                        status: crate::agent::types::ToolStatus::InProgress,
                        output: None,
                    });
                }
            });
        }
        ChatEvent::ToolCallUpdated { id, status, output } => {
            // Only once the call has finished: an in-progress update repeats,
            // and the papers it names are not settled until it completes.
            if status == crate::agent::types::ToolStatus::Completed
                && let Some(text) = output.as_deref()
            {
                link_papers(chat_state, lib_state, db, paper_ids_from_tool_output(text));
            }
            chat_state.with_mut(|s| {
                if let Some(last) = s.messages.last_mut() {
                    for content in &mut last.content {
                        if let MessageContent::ToolUse {
                            id: tool_id,
                            status: tool_status,
                            output: tool_output,
                            ..
                        } = content
                            && *tool_id == id
                        {
                            *tool_status = status.clone();
                            if output.is_some() {
                                *tool_output = output.clone();
                            }
                            break;
                        }
                    }
                }
            });
        }
        ChatEvent::TurnCompleted => {
            if let Some(session_id) = chat_state.read().current_session_id.clone() {
                let db = db.clone();
                let now = chrono::Utc::now().to_rfc3339();
                spawn(async move {
                    let _ = db.touch_chat_session(&session_id, &now).await;
                });
            }
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Idle;
                for msg in &mut s.messages {
                    for content in &mut msg.content {
                        if let MessageContent::ToolUse { status, .. } = content
                            && matches!(
                                status,
                                crate::agent::types::ToolStatus::Pending
                                    | crate::agent::types::ToolStatus::InProgress
                            )
                        {
                            *status = crate::agent::types::ToolStatus::Completed;
                        }
                    }
                }
            });
        }
        ChatEvent::ModelsAvailable { models, current } => {
            chat_state.with_mut(|s| {
                s.available_models = models;
                s.current_model = current;
            });
        }
        ChatEvent::CommandsAvailable(commands) => {
            chat_state.with_mut(|s| s.commands = commands);
        }
        ChatEvent::SessionList(sessions) => {
            chat_state.with_mut(|s| {
                s.past_sessions = sessions;
                s.show_session_browser = true;
            });
        }
        ChatEvent::AuthRequired { provider_name } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::NeedsAuth;
                s.messages.push(ChatMessage::assistant(vec![MessageContent::Text(
                    format!("Sign in to {provider_name} to get started. Go to Settings > AI Agent and use the Sign in option."),
                )]));
            });
        }
        ChatEvent::PermissionRequest {
            request_id,
            tool_title,
            options,
        } => {
            chat_state.with_mut(|s| {
                if s.messages.last().map(|m| &m.role) != Some(&ChatRole::Assistant) {
                    s.messages.push(ChatMessage::assistant(vec![]));
                }
                if let Some(last) = s.messages.last_mut() {
                    last.content.push(MessageContent::Permission {
                        request_id,
                        tool_title,
                        options,
                        responded: false,
                    });
                }
            });
        }
        ChatEvent::Error(err) => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Error(err.clone());
                s.messages
                    .push(ChatMessage::assistant(vec![MessageContent::Error(err)]));
            });
        }
    }
}

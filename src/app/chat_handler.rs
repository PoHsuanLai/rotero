use dioxus::prelude::*;

use crate::agent::types::{AgentStatus, ChatEvent, ChatMessage, ChatRole, ChatState, MessageContent};

pub fn handle_chat_event(chat_state: &mut Signal<ChatState>, event: ChatEvent) {
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
        ChatEvent::Connected { auth_methods, provider_id, supports_list_sessions } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Connecting;
                s.auth_methods = auth_methods;
                s.active_provider_id = provider_id;
                s.supports_list_sessions = supports_list_sessions;
            });
        }
        ChatEvent::SessionCreated => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Idle;
                s.session_active = true;
            });
        }
        ChatEvent::UserMessage(text) => {
            chat_state.with_mut(|s| {
                s.messages.push(ChatMessage {
                    role: ChatRole::User,
                    content: vec![MessageContent::Text(text)],
                    timestamp: chrono::Utc::now(),
                });
            });
        }
        ChatEvent::TextDelta(text) => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Streaming;
                if let Some(last) = s.messages.last_mut() {
                    if last.role == ChatRole::Assistant {
                        if let Some(MessageContent::Text(t)) = last.content.last_mut() {
                            t.push_str(&text);
                        } else {
                            last.content.push(MessageContent::Text(text));
                        }
                        return;
                    }
                }
                s.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: vec![MessageContent::Text(text)],
                    timestamp: chrono::Utc::now(),
                });
            });
        }
        ChatEvent::ToolCallStarted { id, title } => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::ToolCall(title.clone());
                if s.messages.last().map(|m| &m.role) != Some(&ChatRole::Assistant) {
                    s.messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: vec![],
                        timestamp: chrono::Utc::now(),
                    });
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
            chat_state.with_mut(|s| {
                if let Some(last) = s.messages.last_mut() {
                    for content in &mut last.content {
                        if let MessageContent::ToolUse {
                            id: tool_id,
                            status: tool_status,
                            output: tool_output,
                            ..
                        } = content
                        {
                            if *tool_id == id {
                                *tool_status = status.clone();
                                if output.is_some() {
                                    *tool_output = output.clone();
                                }
                                break;
                            }
                        }
                    }
                }
            });
        }
        ChatEvent::TurnCompleted => {
            chat_state.with_mut(|s| {
                s.status = AgentStatus::Idle;
                for msg in &mut s.messages {
                    for content in &mut msg.content {
                        if let MessageContent::ToolUse { status, .. } = content {
                            if matches!(status, crate::agent::types::ToolStatus::Pending | crate::agent::types::ToolStatus::InProgress) {
                                *status = crate::agent::types::ToolStatus::Completed;
                            }
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
                s.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: vec![MessageContent::Text(
                        format!("Sign in to {provider_name} to get started. Go to Settings > AI Agent and use the Sign in option."),
                    )],
                    timestamp: chrono::Utc::now(),
                });
            });
        }
        ChatEvent::PermissionRequest { request_id, tool_title, options } => {
            chat_state.with_mut(|s| {
                if s.messages.last().map(|m| &m.role) != Some(&ChatRole::Assistant) {
                    s.messages.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: vec![],
                        timestamp: chrono::Utc::now(),
                    });
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
                s.messages.push(ChatMessage {
                    role: ChatRole::Assistant,
                    content: vec![MessageContent::Error(err)],
                    timestamp: chrono::Utc::now(),
                });
            });
        }
    }
}

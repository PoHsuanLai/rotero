use dioxus::prelude::*;

use crate::agent::types::{
    ChatMessage, ChatRequest, ChatRole, ChatState, MessageContent, ToolStatus,
};
use crate::ui::chat_panel::AgentChannel;

#[component]
pub(crate) fn ChatMessageBubble(message: ChatMessage) -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let agent_channel = use_context::<AgentChannel>();
    let is_user = message.role == ChatRole::User;
    let bubble_class = if is_user {
        "chat-bubble chat-bubble--user"
    } else {
        "chat-bubble chat-bubble--assistant"
    };

    rsx! {
        div { class: "{bubble_class}",
            for (i, content) in message.content.iter().enumerate() {
                match content {
                    MessageContent::Text(text) => {
                        if is_user {
                            rsx! {
                                div { key: "c{i}", class: "chat-text", "{text}" }
                            }
                        } else {
                            rsx! {
                                MarkdownBlock { key: "c{i}", text: text.clone() }
                            }
                        }
                    },
                    MessageContent::ToolUse { id: _, title, status, output } => {
                        let (icon_class, status_class) = match status {
                            ToolStatus::Pending | ToolStatus::InProgress =>
                                ("bi bi-clock", "chat-tool-call--running"),
                            ToolStatus::Completed =>
                                ("bi bi-check2", "chat-tool-call--done"),
                            ToolStatus::Failed =>
                                ("bi bi-x-lg", "chat-tool-call--failed"),
                        };
                        rsx! {
                            div { key: "c{i}", class: "chat-tool-call {status_class}",
                                i { class: "{icon_class} chat-tool-icon" }
                                span { class: "chat-tool-name", "{title}" }
                            }
                            if let Some(out) = output {
                                div { key: "c{i}-out", class: "chat-tool-output",
                                    pre { "{out}" }
                                }
                            }
                        }
                    },
                    MessageContent::Error(err) => rsx! {
                        div { key: "c{i}", class: "chat-error",
                            i { class: "bi bi-exclamation-triangle chat-error-icon" }
                            span { "{err}" }
                        }
                    },
                    MessageContent::Permission { request_id, tool_title, options, responded } => {
                        if *responded {
                            rsx! {
                                div { key: "c{i}", class: "chat-tool-call chat-tool-call--done",
                                    i { class: "bi bi-check2 chat-tool-icon" }
                                    span { class: "chat-tool-name", "{tool_title}" }
                                }
                            }
                        } else {
                            let req_id = request_id.clone();
                            let opts = options.clone();
                            rsx! {
                                div { key: "c{i}", class: "chat-permission",
                                    span { class: "chat-permission-title", "Allow {tool_title}?" }
                                    div { class: "chat-permission-buttons",
                                        for (opt_id, label) in opts.iter() {
                                            {
                                                let opt_id = opt_id.clone();
                                                let req_id = req_id.clone();
                                                rsx! {
                                                    button {
                                                        key: "{opt_id}",
                                                        class: "btn btn--primary chat-permission-btn",
                                                        onclick: move |_| {
                                                            agent_channel.send(ChatRequest::PermissionResponse {
                                                                request_id: req_id.clone(),
                                                                option_id: opt_id.clone(),
                                                            });
                                                            chat_state.with_mut(|s| {
                                                                for msg in &mut s.messages {
                                                                    for content in &mut msg.content {
                                                                        if let MessageContent::Permission { responded, .. } = content {
                                                                            *responded = true;
                                                                        }
                                                                    }
                                                                }
                                                            });
                                                        },
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}

#[component]
fn MarkdownBlock(text: String) -> Element {
    let html = crate::ui::markdown::md_to_html(&text);

    rsx! {
        div {
            class: "chat-md",
            dangerous_inner_html: "{html}",
        }
    }
}

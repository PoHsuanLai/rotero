use dioxus::prelude::*;

use crate::agent::types::{AgentStatus, ChatRequest, ChatState};
use crate::state::app_state::{LibraryState, PdfTabManager};

use super::{do_send, get_context_paper_title, AgentChannel};
use super::message::ChatMessageBubble;
use super::resize_handle::ResizeHandle;

#[component]
pub fn ChatPanel() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let agent_channel = use_context::<AgentChannel>();
    let lib_state = use_context::<Signal<LibraryState>>();
    let tab_mgr = use_context::<Signal<PdfTabManager>>();

    let status = chat_state.read().status.clone();
    let messages = chat_state.read().messages.clone();

    // Auto-scroll: set up a MutationObserver once to scroll on any DOM change
    use_effect(move || {
        spawn(async {
            let _ = dioxus::document::eval(r#"
                (function() {
                    let el = document.querySelector('.chat-messages');
                    if (!el || el._autoScroll) return;
                    el._autoScroll = true;
                    new MutationObserver(() => {
                        el.scrollTop = el.scrollHeight;
                    }).observe(el, { childList: true, subtree: true, characterData: true });
                })()
            "#);
        });
    });
    let paper_title = get_context_paper_title(&lib_state.read(), &tab_mgr.read());
    let has_context = paper_title.is_some();
    let paper_title_display = paper_title.unwrap_or_default();
    let active_provider = chat_state.read().active_provider_id.clone();
    let provider_name = crate::agent::types::AGENT_PROVIDERS
        .iter()
        .find(|p| p.id == active_provider)
        .map(|p| p.name)
        .unwrap_or("AI Chat");
    let available_models = chat_state.read().available_models.clone();
    let current_model = chat_state.read().current_model.clone();
    let show_commands = chat_state.read().show_command_picker;
    let commands = chat_state.read().commands.clone();
    let show_sessions = chat_state.read().show_session_browser;
    let past_sessions = chat_state.read().past_sessions.clone();

    let status_text = match &status {
        AgentStatus::Idle => "Ready",
        AgentStatus::Connecting => "Connecting...",
        AgentStatus::Streaming => "Thinking...",
        AgentStatus::ToolCall(name) => name.as_str(),
        AgentStatus::NeedsAuth => "Sign in required",
        AgentStatus::Error(_) => "Error",
        AgentStatus::NotInstalled => "Not installed",
    };

    let is_busy = matches!(
        status,
        AgentStatus::Connecting | AgentStatus::Streaming | AgentStatus::ToolCall(_)
    );

    // Filter commands based on input text after /
    let input_text = chat_state.read().input_text.clone();
    let filtered_commands: Vec<_> = if show_commands {
        let query = input_text.strip_prefix('/').unwrap_or("").to_lowercase();
        commands
            .iter()
            .filter(|c| query.is_empty() || c.name.to_lowercase().contains(&query))
            .cloned()
            .collect()
    } else {
        vec![]
    };

    rsx! {
        div { class: "chat-panel",
            ResizeHandle { target: "chat" }

            // Header
            div { class: "chat-header",
                div { class: "chat-header-left",
                    span { class: "chat-title", "{provider_name}" }
                    span {
                        class: "chat-status",
                        class: if is_busy { "chat-status--busy" } else { "" },
                        "{status_text}"
                    }
                }
                div { class: "chat-header-right",
                    // New chat button
                    button {
                        class: "chat-header-btn",
                        title: "New chat",
                        onclick: move |_| {
                            chat_state.with_mut(|s| {
                                s.messages.clear();
                                s.status = AgentStatus::Idle;
                            });
                        },
                        i { class: "bi bi-plus-lg" }
                    }
                    // Past sessions button (only if agent supports it)
                    if chat_state.read().supports_list_sessions {
                        button {
                            class: "chat-header-btn",
                            title: "Past chats",
                            onclick: move |_| {
                                agent_channel.send(ChatRequest::ListSessions);
                            },
                            i { class: "bi bi-clock" }
                        }
                    }
                    // Close button
                    button {
                        class: "chat-close",
                        onclick: move |_| {
                            chat_state.with_mut(|s| s.panel_open = false);
                        },
                        "\u{00D7}"
                    }
                }
            }

            // Session browser overlay
            if show_sessions {
                div { class: "chat-session-browser",
                    div { class: "chat-session-header",
                        span { class: "chat-session-title", "Past chats" }
                        button {
                            class: "chat-header-btn",
                            onclick: move |_| {
                                chat_state.with_mut(|s| s.show_session_browser = false);
                            },
                            "\u{00D7}"
                        }
                    }
                    div { class: "chat-session-list",
                        if past_sessions.is_empty() {
                            div { class: "chat-empty",
                                p { "No past chats found." }
                            }
                        } else {
                            for session in past_sessions.iter() {
                                {
                                    let sid = session.session_id.clone();
                                    let session_cwd = session.cwd.clone();
                                    let title = session.title.clone().unwrap_or_else(|| "Untitled".into());
                                    let updated = session.updated_at.clone().unwrap_or_default();
                                    rsx! {
                                        button {
                                            key: "{sid}",
                                            class: "chat-session-item",
                                            onclick: move |_| {
                                                agent_channel.send(ChatRequest::LoadSession {
                                                    session_id: sid.clone(),
                                                    cwd: session_cwd.clone(),
                                                });
                                                chat_state.with_mut(|s| {
                                                    s.messages.clear();
                                                    s.show_session_browser = false;
                                                    s.status = AgentStatus::Connecting;
                                                });
                                            },
                                            div { class: "chat-session-item-title", "{title}" }
                                            div { class: "chat-session-item-date", "{updated}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Messages
            div {
                class: "chat-messages",
                if messages.is_empty() && !show_sessions {
                    div { class: "chat-empty",
                        p { "Ask Claude about your papers." }
                        if has_context {
                            p { class: "chat-empty-hint",
                                "Context: {paper_title_display}"
                            }
                        }
                    }
                } else {
                    for (i, msg) in messages.iter().enumerate() {
                        ChatMessageBubble { key: "{i}", message: msg.clone() }
                    }
                }
            }

            // Paper context badge
            if has_context {
                div { class: "chat-context-badge",
                    span { class: "chat-context-text", "Discussing: {paper_title_display}" }
                }
            }

            // Slash command picker
            if show_commands && !filtered_commands.is_empty() {
                div { class: "chat-command-picker",
                    for cmd in filtered_commands.iter() {
                        {
                            let name = cmd.name.clone();
                            let hint = cmd.hint.clone().unwrap_or_default();
                            rsx! {
                                button {
                                    key: "{name}",
                                    class: "chat-command-item",
                                    onclick: move |_| {
                                        let text = if hint.is_empty() {
                                            format!("/{name} ")
                                        } else {
                                            format!("/{name} ")
                                        };
                                        chat_state.with_mut(|s| {
                                            s.input_text = text;
                                            s.show_command_picker = false;
                                        });
                                    },
                                    span { class: "chat-command-name", "/{name}" }
                                    span { class: "chat-command-desc", "{cmd.description}" }
                                }
                            }
                        }
                    }
                }
            }

            // Model selector + Input area
            if !available_models.is_empty() {
                div { class: "chat-input-meta",
                    select {
                        class: "chat-model-select",
                        value: "{current_model}",
                        onchange: move |e| {
                            let model_id = e.value();
                            chat_state.with_mut(|s| s.current_model = model_id.clone());
                            agent_channel.send(ChatRequest::SetModel { model_id });
                        },
                        for model in available_models.iter() {
                            option {
                                value: "{model.id}",
                                selected: model.id == current_model,
                                "{model.name}"
                            }
                        }
                    }
                }
            }
            div { class: "chat-input-area",
                textarea {
                    class: "chat-input",
                    placeholder: "Ask about your papers... (/ for commands)",
                    value: "{chat_state.read().input_text}",
                    disabled: is_busy,
                    rows: 3,
                    oninput: move |e| {
                        let val = e.value();
                        chat_state.with_mut(|s| {
                            s.input_text = val.clone();
                            // Show command picker when typing /
                            s.show_command_picker = val.starts_with('/') && !val.contains(' ');
                        });
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !e.modifiers().shift() {
                            e.prevent_default();
                            do_send(&mut chat_state, &agent_channel, &lib_state, &tab_mgr);
                        }
                        if e.key() == Key::Escape {
                            chat_state.with_mut(|s| s.show_command_picker = false);
                        }
                    },
                }
                button {
                    class: "chat-send-btn",
                    class: if is_busy { "chat-send-btn--stop" } else { "" },
                    onclick: move |_| {
                        if is_busy {
                            agent_channel.send(ChatRequest::Cancel);
                            chat_state.with_mut(|s| s.status = AgentStatus::Idle);
                        } else {
                            do_send(&mut chat_state, &agent_channel, &lib_state, &tab_mgr);
                        }
                    },
                    if is_busy {
                        i { class: "bi bi-stop-fill" }
                    } else {
                        i { class: "bi bi-arrow-up" }
                    }
                }
            }
        }
    }
}

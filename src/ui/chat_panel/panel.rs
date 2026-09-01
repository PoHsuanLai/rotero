use dioxus::prelude::*;

use crate::agent::types::{AgentStatus, ChatRequest, ChatState};
use crate::state::app_state::{LibraryState, PdfTabManager};

use super::message::ChatMessageBubble;
use super::resize_handle::ResizeHandle;
use super::{AgentChannel, do_send};

#[component]
pub fn ChatPanel() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let agent_channel = use_context::<AgentChannel>();
    let db = use_context::<rotero_db::Database>();
    let db_key = db.clone();
    let lib_state = use_context::<Signal<LibraryState>>();
    let tab_mgr = use_context::<Signal<PdfTabManager>>();

    let status = chat_state.read().status.clone();
    let messages = chat_state.read().messages.clone();

    use_effect(move || {
        spawn(async {
            let _ = dioxus::document::eval(
                r#"
                (function() {
                    const el = document.querySelector('.chat-messages');
                    if (!el || el._autoScroll === 2) return;
                    if (el._autoScrollObs) {
                        try { el._autoScrollObs.disconnect(); } catch (e) {}
                    }
                    el._autoScroll = 2;
                    const THRESHOLD = 40;
                    const gap = () => el.scrollHeight - el.scrollTop - el.clientHeight;
                    let pinned = gap() < THRESHOLD;
                    el.addEventListener('scroll', () => { pinned = gap() < THRESHOLD; }, { passive: true });
                    const obs = new MutationObserver(() => {
                        if (pinned) el.scrollTop = el.scrollHeight;
                    });
                    obs.observe(el, { childList: true, subtree: true, characterData: true });
                    el._autoScrollObs = obs;
                })()
            "#,
            );
        });
    });
    // What the conversation is about, falling back to what the next message
    // would be about before one has started.
    let subject = chat_state
        .read()
        .current_subject
        .clone()
        .or_else(|| super::current_subject(&lib_state.read(), &tab_mgr.read()));
    let subject_name = subject
        .as_ref()
        .map(|s| super::subject_label(s, &lib_state.read()));
    let has_context = subject_name.is_some();
    let paper_title_display = subject_name.unwrap_or_default();
    let pending_switch = chat_state.read().pending_switch.clone();
    let provider_name = {
        let name = chat_state.read().active_provider_name.clone();
        if name.is_empty() {
            "AI Chat".into()
        } else {
            name
        }
    };
    let available_models = chat_state.read().available_models.clone();
    let current_model = chat_state.read().current_model.clone();
    let show_commands = chat_state.read().show_command_picker;
    let commands = chat_state.read().commands.clone();
    let show_sessions = chat_state.read().show_session_browser;
    let past_sessions = chat_state.read().past_sessions.clone();
    let browse_all = chat_state.read().browse_all_sessions;

    // Labelled from our own record rather than the agent's. The agent titles a
    // session after its first user message, which for these is a synthetic
    // startup entry — so every one of its titles reads "/model".
    // Only the database read is deferred. Resolving a subject to its title reads
    // the library, which is reactive state: doing that inside the future would
    // not register as a dependency, so the labels would never refresh.
    let described = use_resource({
        let db = db.clone();
        move || {
            let db = db.clone();
            async move {
                (
                    db.all_chat_sessions().await.unwrap_or_default(),
                    db.all_chat_session_subjects().await.unwrap_or_default(),
                )
            }
        }
    });

    // The list is about what is open, so it shows that subject's conversations
    // unless widened. Filtering waits for the record to load: without it every
    // row would be hidden for the frame before it arrives.
    let visible_sessions: Vec<_> = match (&*described.read(), browse_all, subject.as_ref()) {
        (Some((rows, subjects)), false, Some(current)) => past_sessions
            .iter()
            .filter(|s| {
                rows.iter()
                    .find(|r| r.session_id == s.session_id)
                    .and_then(|r| super::subject_of_row(r, subjects))
                    .is_some_and(|subj| subj == *current)
            })
            .cloned()
            .collect(),
        _ => past_sessions.clone(),
    };

    let hidden_count = past_sessions.len().saturating_sub(visible_sessions.len());

    let tool_status = match &status {
        AgentStatus::ToolCall(name) => Some(super::rotero_tools::humanize_tool_title(name)),
        _ => None,
    };
    let status_text = match &status {
        AgentStatus::Idle => "Ready",
        AgentStatus::Connecting => "Connecting...",
        AgentStatus::Streaming => "Thinking...",
        AgentStatus::ToolCall(_) => tool_status.as_deref().unwrap_or("Working"),
        AgentStatus::NeedsAuth => "Sign in required",
        AgentStatus::Error(_) => "Error",
    };

    let is_busy = matches!(
        status,
        AgentStatus::Connecting | AgentStatus::Streaming | AgentStatus::ToolCall(_)
    );

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
        super::SubjectFollower {}

        if let Some(switch) = pending_switch {
            {
                let subject = switch.subject.clone();
                let db_switch = db.clone();
                rsx! {
                    crate::ui::components::confirm_dialog::ConfirmDialog {
                        title: "Switch conversation?".to_string(),
                        message: format!(
                            "Continue the conversation about {}? This chat stays saved.",
                            switch.label,
                        ),
                        confirm_label: "Switch".to_string(),
                        on_confirm: move |_| {
                            super::switch_to(&mut chat_state, &agent_channel, &db_switch, subject.clone());
                        },
                        on_cancel: move |_| {
                            chat_state.with_mut(|s| {
                                // Remembered as declined so the follower does
                                // not re-ask about the same subject on every
                                // render; a different subject asks afresh.
                                s.declined_subject = s.pending_switch.take().map(|p| p.subject);
                            });
                        },
                    }
                }
            }
        }

        div { class: "chat-panel",
            ResizeHandle { target: "chat" }

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
                    button {
                        class: "chat-close",
                        onclick: move |_| {
                            chat_state.with_mut(|s| s.panel_open = false);
                        },
                        "\u{00D7}"
                    }
                }
            }

            if show_sessions {
                div { class: "chat-session-browser",
                    div { class: "chat-session-header",
                        span { class: "chat-session-title",
                            if browse_all { "All chats" } else { "Chats about this" }
                        }
                        // Only worth offering when it would change the list.
                        if hidden_count > 0 || browse_all {
                            button {
                                class: "chat-session-scope",
                                onclick: move |_| {
                                    chat_state.with_mut(|s| {
                                        s.browse_all_sessions = !s.browse_all_sessions;
                                    });
                                },
                                if browse_all {
                                    "Only this"
                                } else {
                                    "Show all ({hidden_count})"
                                }
                            }
                        }
                        button {
                            class: "chat-header-btn",
                            onclick: move |_| {
                                chat_state.with_mut(|s| s.show_session_browser = false);
                            },
                            "\u{00D7}"
                        }
                    }
                    div { class: "chat-session-list",
                        if visible_sessions.is_empty() {
                            div { class: "chat-empty",
                                p {
                                    if past_sessions.is_empty() {
                                        "No past chats found."
                                    } else {
                                        "No past chats about this yet."
                                    }
                                }
                            }
                        } else {
                            for session in visible_sessions.iter() {
                                {
                                    let sid = session.session_id.clone();
                                    let session_cwd = session.cwd.clone();
                                    // Falls back to the agent's own title only
                                    // until the record loads; ours is better
                                    // but arrives a frame later.
                                    let (title, about) = match &*described.read() {
                                        Some((rows, subjects)) => {
                                            let known =
                                                rows.iter().find(|r| r.session_id == sid);
                                            let about = known.and_then(|r| {
                                                super::subject_of_row(r, subjects).map(|subj| {
                                                    super::subject_label(&subj, &lib_state.read())
                                                })
                                            });
                                            let title = known
                                                .and_then(|r| r.summary.clone())
                                                .unwrap_or_else(|| {
                                                    let when = known
                                                        .map(|r| r.last_used_at.as_str())
                                                        .or(session.updated_at.as_deref())
                                                        .unwrap_or_default();
                                                    super::unlabelled_title(about.as_deref(), when)
                                                });
                                            (title, about)
                                        }
                                        None => (
                                            session
                                                .title
                                                .clone()
                                                .unwrap_or_else(|| "Loading…".into()),
                                            None,
                                        ),
                                    };
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
                                            if let Some(about) = about {
                                                div { class: "chat-session-item-about", "About: {about}" }
                                            }
                                            div { class: "chat-session-item-date", "{updated}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div {
                class: "chat-messages",
                if messages.is_empty() && !show_sessions {
                    div { class: "chat-empty",
                        p { "Ask {provider_name} about your papers." }
                        if has_context {
                            p { class: "chat-empty-hint",
                                "Context: {paper_title_display}"
                            }
                        }
                    }
                } else {
                    for (i, msg) in messages.iter().enumerate() {
                        if !msg.hidden {
                            ChatMessageBubble { key: "{i}", message: msg.clone() }
                        }
                    }
                }
            }

            if has_context {
                div { class: "chat-context-badge",
                    span { class: "chat-context-text", "About: {paper_title_display}" }
                }
            }

            if show_commands && !filtered_commands.is_empty() {
                div { class: "chat-command-picker",
                    for cmd in filtered_commands.iter() {
                        {
                            let name = cmd.name.clone();
                            let _hint = cmd.hint.clone().unwrap_or_default();
                            rsx! {
                                button {
                                    key: "{name}",
                                    class: "chat-command-item",
                                    onclick: move |_| {
                                        let text = format!("/{name} ");
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
                    onfocusin: crate::ui::keybindings::editable_focus_in,
                    onfocusout: crate::ui::keybindings::editable_focus_out,
                    oninput: move |e| {
                        let val = e.value();
                        chat_state.with_mut(|s| {
                            s.input_text = val.clone();
                            s.show_command_picker = val.starts_with('/') && !val.contains(' ');
                        });
                    },
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !e.modifiers().shift() {
                            e.prevent_default();
                            do_send(&mut chat_state, &agent_channel, &lib_state, &tab_mgr, &db_key);
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
                            do_send(&mut chat_state, &agent_channel, &lib_state, &tab_mgr, &db);
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

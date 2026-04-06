use dioxus::prelude::*;

use crate::agent::types::{
    AgentStatus, ChatMessage, ChatRequest, ChatRole, ChatState, MessageContent, ToolStatus,
};
use crate::state::app_state::{LibraryState, PdfTabManager};

/// Channel wrapper for sending requests to the ACP agent thread.
#[derive(Clone, Copy)]
pub struct AgentChannel {
    pub inner: Signal<Option<std::sync::mpsc::Sender<ChatRequest>>>,
}

impl AgentChannel {
    pub fn send(&self, req: ChatRequest) {
        if let Some(tx) = self.inner.read().as_ref() {
            tracing::info!("AgentChannel: sending request");
            let _ = tx.send(req);
        } else {
            tracing::warn!("AgentChannel: no sender available");
        }
    }
}

fn get_active_paper_id(lib_state: &LibraryState, tab_mgr: &PdfTabManager) -> Option<String> {
    tab_mgr
        .active_tab_id
        .and_then(|tid| tab_mgr.tabs.iter().find(|t| t.id == tid))
        .and_then(|t| t.paper_id.clone())
        .or_else(|| lib_state.selected_paper_id.clone())
}

fn build_paper_context(lib_state: &LibraryState, tab_mgr: &PdfTabManager) -> Option<String> {
    let paper_id = get_active_paper_id(lib_state, tab_mgr)?;
    let paper = lib_state.papers.iter().find(|p| p.id.as_deref() == Some(paper_id.as_str()))?;

    Some(format!(
        "I'm currently looking at this paper in my library:\n\
         Title: {}\nAuthors: {}\nYear: {}\nDOI: {}\nPaper ID: {}\n\
         You can use the rotero MCP tools to search my library, \
         read this paper's annotations, extract PDF text, etc.",
        paper.title,
        paper.authors.join(", "),
        paper.year.map(|y| y.to_string()).unwrap_or_default(),
        paper.doi.as_deref().unwrap_or(""),
        paper_id,
    ))
}

fn get_context_paper_title(
    lib_state: &LibraryState,
    tab_mgr: &PdfTabManager,
) -> Option<String> {
    let paper_id = get_active_paper_id(lib_state, tab_mgr)?;
    lib_state
        .papers
        .iter()
        .find(|p| p.id.as_deref() == Some(paper_id.as_str()))
        .map(|p| p.title.clone())
}

fn do_send(
    chat_state: &mut Signal<ChatState>,
    agent_channel: &AgentChannel,
    lib_state: &Signal<LibraryState>,
    tab_mgr: &Signal<PdfTabManager>,
) {
    let input = chat_state.read().input_text.trim().to_string();
    if input.is_empty() {
        return;
    }

    chat_state.with_mut(|s| {
        s.messages.push(ChatMessage {
            role: ChatRole::User,
            content: vec![MessageContent::Text(input.clone())],
            timestamp: chrono::Utc::now(),
        });
        s.input_text.clear();
        s.status = AgentStatus::Streaming;
        s.show_command_picker = false;
    });

    let paper_context = build_paper_context(&lib_state.read(), &tab_mgr.read());

    agent_channel.send(ChatRequest::SendMessage {
        prompt: input,
        paper_context,
    });
}

/// Chat toggle button for the library header / PDF tab bar.
#[component]
pub fn ChatToggleButton() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let is_open = chat_state.read().panel_open;

    rsx! {
        button {
            class: "btn btn--ghost",
            class: if is_open { "chat-toggle-btn--active" } else { "" },
            title: "AI Chat",
            onclick: move |_| {
                chat_state.with_mut(|s| s.panel_open = !s.panel_open);
            },
            i { class: "bi bi-chat-dots" }
        }
    }
}

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

#[component]
fn ChatMessageBubble(message: ChatMessage) -> Element {
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
                                                            // Mark as responded
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

/// Renders markdown text as HTML.
#[component]
fn MarkdownBlock(text: String) -> Element {
    let html = md_to_html(&text);

    rsx! {
        div {
            class: "chat-md",
            dangerous_inner_html: "{html}",
        }
    }
}

fn md_to_html(text: &str) -> String {
    use pulldown_cmark::{Options, Parser, html};

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(text, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    html_output
}

/// Drag-resize handle for sidebars.
#[component]
pub fn ResizeHandle(target: String) -> Element {
    let handle_class = format!("{target}-resize-handle");

    rsx! {
        div {
            class: "{handle_class}",
            onmousedown: move |e| {
                e.prevent_default();
                let target = target.clone();
                let start_x = e.client_coordinates().x;
                let selector = if target == "detail" {
                    ".paper-detail".to_string()
                } else {
                    format!(".{target}-panel")
                };
                spawn(async move {
                    let js = format!(
                        r#"
                        (function() {{
                            var panel = document.querySelector('{selector}');
                            if (!panel) return;
                            var startX = {start_x};
                            var startW = panel.offsetWidth;
                            function onMove(e) {{
                                var diff = startX - e.clientX;
                                var newW = Math.max(280, Math.min(600, startW + diff));
                                panel.style.width = newW + 'px';
                                panel.style.minWidth = newW + 'px';
                            }}
                            function onUp() {{
                                document.removeEventListener('mousemove', onMove);
                                document.removeEventListener('mouseup', onUp);
                                document.body.style.cursor = '';
                                document.body.style.userSelect = '';
                            }}
                            document.body.style.cursor = 'col-resize';
                            document.body.style.userSelect = 'none';
                            document.addEventListener('mousemove', onMove);
                            document.addEventListener('mouseup', onUp);
                        }})()
                        "#
                    );
                    let _ = dioxus::document::eval(&js);
                });
            },
        }
    }
}

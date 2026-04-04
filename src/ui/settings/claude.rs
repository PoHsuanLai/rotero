use dioxus::prelude::*;

use crate::agent::types::{AgentStatus, ChatRequest, ChatState, AGENT_PROVIDERS};
use crate::sync::engine::SyncConfig;
use crate::ui::chat_panel::AgentChannel;

#[component]
pub fn AgentSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let chat_state = use_context::<Signal<ChatState>>();
    let agent_channel = use_context::<AgentChannel>();

    let selected_provider = config.read().agent_provider.clone();
    let connected_provider = chat_state.read().active_provider_id.clone();
    let agent_connected = chat_state.read().session_active;
    let agent_status = chat_state.read().status.clone();
    let auth_methods = chat_state.read().auth_methods.clone();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "AI Agent" }

            // Provider cards
            div { class: "agent-provider-grid",
                for provider in AGENT_PROVIDERS.iter() {
                    {
                        let pid = provider.id;
                        let is_selected = selected_provider == pid;
                        let is_connected = agent_connected && connected_provider == pid;
                        let card_class = match (is_selected, is_connected) {
                            (true, true) => "agent-provider-card agent-provider-card--selected agent-provider-card--connected",
                            (true, false) => "agent-provider-card agent-provider-card--selected",
                            (false, true) => "agent-provider-card agent-provider-card--connected",
                            (false, false) => "agent-provider-card",
                        };
                        rsx! {
                            button {
                                key: "{pid}",
                                class: "{card_class}",
                                onclick: move |_| {
                                    config.with_mut(|c| c.agent_provider = pid.to_string());
                                    let _ = config.read().save();
                                    agent_channel.send(ChatRequest::SwitchAgent {
                                        provider_id: pid.to_string(),
                                    });
                                },
                                div { class: "agent-provider-name", "{provider.name}" }
                                div { class: "agent-provider-desc", "{provider.description}" }
                                if is_connected {
                                    div { class: "agent-provider-badge agent-provider-badge--connected",
                                        "Connected"
                                    }
                                } else if is_selected && matches!(agent_status, AgentStatus::Connecting) {
                                    div { class: "agent-provider-badge",
                                        "Connecting..."
                                    }
                                } else if is_selected && matches!(agent_status, AgentStatus::Error(_)) {
                                    div { class: "agent-provider-badge agent-provider-badge--error",
                                        "Error"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Auth methods from ACP — shown for the selected provider when connected
            if !auth_methods.is_empty() && selected_provider == connected_provider {
                div { class: "settings-field",
                    span { class: "settings-field-label", "Sign in" }
                    div { class: "settings-field-control settings-auth-buttons",
                        for method in auth_methods.iter() {
                            {
                                let name = method.name.clone();
                                let desc = method.description.clone().unwrap_or_default();
                                let cmd = method.terminal_command.clone();
                                let args = method.terminal_args.clone();
                                let btn_class = if agent_connected {
                                    "btn btn--primary"
                                } else {
                                    "btn btn--secondary"
                                };
                                rsx! {
                                    button {
                                        key: "{name}",
                                        class: "{btn_class}",
                                        title: "{desc}",
                                        onclick: move |_| {
                                            if let Some(command) = &cmd {
                                                let _ = std::process::Command::new(command)
                                                    .args(&args)
                                                    .spawn();
                                            }
                                        },
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            p { class: "settings-hint",
                "Uses the Agent Client Protocol (ACP). Auth is handled by each provider."
            }
        }
    }
}

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

            // API key input for the selected provider
            if let Some(provider) = AGENT_PROVIDERS.iter().find(|p| p.id == selected_provider) {
                if !provider.env_keys.is_empty() {
                    for env_key in provider.env_keys.iter() {
                        {
                            let key = *env_key;
                            let hint = provider.env_hint;
                            let current_val = config
                                .read()
                                .agent_env_vars
                                .get(key)
                                .cloned()
                                .unwrap_or_default();
                            // Mask the value for display
                            let display_val = if current_val.is_empty() {
                                String::new()
                            } else if current_val.len() > 8 {
                                format!("{}...{}", &current_val[..4], &current_val[current_val.len()-4..])
                            } else {
                                "*".repeat(current_val.len())
                            };
                            let has_val = !current_val.is_empty();
                            rsx! {
                                div { class: "settings-field",
                                    span { class: "settings-field-label", "{key}" }
                                    div { class: "settings-field-control",
                                        if has_val {
                                            div { class: "agent-key-display",
                                                span { class: "agent-key-value", "{display_val}" }
                                                button {
                                                    class: "btn btn--secondary agent-key-clear",
                                                    onclick: move |_| {
                                                        config.with_mut(|c| {
                                                            c.agent_env_vars.remove(key);
                                                        });
                                                        let _ = config.read().save();
                                                    },
                                                    "Clear"
                                                }
                                            }
                                        } else {
                                            input {
                                                class: "input input--sm",
                                                r#type: "password",
                                                placeholder: "{hint}",
                                                onchange: move |e| {
                                                    let val = e.value();
                                                    if !val.is_empty() {
                                                        config.with_mut(|c| {
                                                            c.agent_env_vars
                                                                .insert(key.to_string(), val);
                                                        });
                                                        let _ = config.read().save();
                                                        // Reconnect to pick up the new key
                                                        let pid = config.read().agent_provider.clone();
                                                        agent_channel.send(ChatRequest::SwitchAgent {
                                                            provider_id: pid,
                                                        });
                                                    }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Auth methods from ACP (for providers like Claude that use terminal-auth)
            if !auth_methods.is_empty() && config.read().agent_provider == connected_provider {
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
                "Uses the Agent Client Protocol (ACP). Switch providers anytime."
            }
        }
    }
}

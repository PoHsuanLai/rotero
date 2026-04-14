use dioxus::prelude::*;

use crate::agent::types::{AGENT_PROVIDERS, AgentStatus, ChatRequest, ChatState};
use crate::sync::engine::SyncConfig;
use crate::ui::chat_panel::AgentChannel;
use crate::ui::components::settings_field::SettingsField;
use crate::ui::helpers::save_config;

#[component]
pub fn AgentSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let chat_state = use_context::<Signal<ChatState>>();
    let agent_channel = use_context::<AgentChannel>();

    let mut pending_provider = use_signal(|| config.read().agent.agent_provider.clone());
    let saved_provider = config.read().agent.agent_provider.clone();
    let connected_provider = chat_state.read().active_provider_id.clone();
    let agent_connected = chat_state.read().session_active;
    let agent_status = chat_state.read().status.clone();
    let auth_methods = chat_state.read().auth_methods.clone();

    let has_unsaved = *pending_provider.read() != saved_provider;
    let auth_methods_for_btn = auth_methods.clone();
    let mut selected_method = use_signal(|| 0usize);
    let mut api_key_input = use_signal(String::new);

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "AI Agent" }

            div { class: "agent-provider-grid",
                for provider in AGENT_PROVIDERS.iter() {
                    {
                        let pid = provider.id;
                        let is_pending = *pending_provider.read() == pid;
                        let is_connected = agent_connected && connected_provider == pid;
                        let card_class = match (is_pending, is_connected) {
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
                                    pending_provider.set(pid.to_string());
                                },
                                div { class: "agent-provider-name", "{provider.name}" }
                                div { class: "agent-provider-desc", "{provider.description}" }
                                if is_connected {
                                    div { class: "agent-provider-badge agent-provider-badge--connected",
                                        "Connected"
                                    }
                                } else if is_pending && !has_unsaved && matches!(agent_status, AgentStatus::Connecting) {
                                    div { class: "agent-provider-badge",
                                        "Connecting..."
                                    }
                                } else if is_pending && !has_unsaved && matches!(agent_status, AgentStatus::NeedsAuth) {
                                    div { class: "agent-provider-badge agent-provider-badge--auth",
                                        "Sign in required"
                                    }
                                } else if is_pending && !has_unsaved && matches!(agent_status, AgentStatus::Error(_)) {
                                    div { class: "agent-provider-badge agent-provider-badge--error",
                                        "Error"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if has_unsaved {
                SettingsField { label: "",
                    button {
                        class: "btn btn--primary",
                        onclick: move |_| {
                            let pid = pending_provider.read().clone();
                            save_config(&mut config, |c| c.agent.agent_provider = pid.clone());
                            agent_channel.send(ChatRequest::SwitchAgent {
                                provider_id: pid,
                            });
                        },
                        "Save & Connect"
                    }
                }
            }

            if !auth_methods.is_empty() && *pending_provider.read() == connected_provider {
                {
                    let sel_idx = *selected_method.read();
                    let selected_is_api_key = auth_methods.get(sel_idx).map(|m| m.is_api_key).unwrap_or(false);
                    let selected_env_var = auth_methods.get(sel_idx).and_then(|m| m.api_key_env_var.clone()).unwrap_or_default();
                    let current_key = if selected_is_api_key {
                        config.read().agent.agent_api_keys.get(&selected_env_var).cloned().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let has_key = !current_key.is_empty();
                    let masked_key = if current_key.len() > 8 {
                        format!("{}...{}", &current_key[..4], &current_key[current_key.len()-4..])
                    } else if has_key {
                        "*".repeat(current_key.len())
                    } else {
                        String::new()
                    };

                    rsx! {
                        SettingsField { label: "Account",
                            div { class: "agent-auth-row",
                                select {
                                    class: "select",
                                    onchange: move |e| {
                                        if let Ok(idx) = e.value().parse::<usize>() {
                                            selected_method.set(idx);
                                        }
                                    },
                                    for (idx, method) in auth_methods.iter().enumerate() {
                                        option {
                                            value: "{idx}",
                                            "{method.name}"
                                        }
                                    }
                                }
                                if !selected_is_api_key {
                                    button {
                                        class: if agent_connected { "btn btn--secondary" } else { "btn btn--primary" },
                                        onclick: move |_| {
                                            let idx = *selected_method.read();
                                            if let Some(method) = auth_methods_for_btn.get(idx) {
                                                if let Some(command) = &method.terminal_command {
                                                    let _ = std::process::Command::new(command)
                                                        .args(&method.terminal_args)
                                                        .spawn();
                                                } else {
                                                    agent_channel.send(ChatRequest::Authenticate {
                                                        method_id: method.id.clone(),
                                                    });
                                                }
                                            }
                                        },
                                        if agent_connected { "Switch" } else { "Sign in" }
                                    }
                                }
                            }
                        }
                        if selected_is_api_key {
                            SettingsField { label: "",
                                div { class: "agent-auth-row",
                                    if has_key {
                                        span { class: "agent-key-masked", "{masked_key}" }
                                        button {
                                            class: "btn btn--secondary",
                                            onclick: move |_| {
                                                save_config(&mut config, |c| {
                                                    c.agent.agent_api_keys.remove(&selected_env_var);
                                                });
                                            },
                                            "Clear"
                                        }
                                    } else {
                                        input {
                                            class: "input input--sm",
                                            r#type: "password",
                                            placeholder: "{selected_env_var}",
                                            value: "{api_key_input.read()}",
                                            oninput: move |e| {
                                                api_key_input.set(e.value());
                                            },
                                        }
                                        button {
                                            class: "btn btn--primary",
                                            disabled: api_key_input.read().is_empty(),
                                            onclick: move |_| {
                                                let val = api_key_input.read().clone();
                                                if !val.is_empty() {
                                                    save_config(&mut config, |c| {
                                                        c.agent.agent_api_keys.insert(selected_env_var.clone(), val);
                                                    });
                                                    api_key_input.set(String::new());
                                                    let pid = config.read().agent.agent_provider.clone();
                                                    agent_channel.send(ChatRequest::SwitchAgent {
                                                        provider_id: pid,
                                                    });
                                                }
                                            },
                                            "Save"
                                        }
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

use std::path::PathBuf;
use std::sync::mpsc;

use super::install::{find_mcp_binary, find_pdfium_path};
use super::types::{
    AgentAuthMethod, AgentModel, ChatEvent, ChatRequest, SlashCommand, ToolStatus,
};
use super::LoopResult;

pub(crate) fn agent_working_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.data_dir().join("com.rotero.Rotero"))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

pub(crate) fn build_mcp_servers_json() -> serde_json::Value {
    #[cfg(feature = "desktop")]
    if let Some(&port) = crate::MCP_HTTP_PORT.get() {
        let url = format!("http://127.0.0.1:{port}/mcp");
        tracing::info!("MCP: using embedded HTTP server at {url}");
        return serde_json::json!([{
            "type": "http",
            "name": "rotero",
            "url": url,
            "headers": [],
        }]);
    }

    let mcp_binary = find_mcp_binary();
    let pdfium_path = find_pdfium_path();

    if let Some(mcp_bin) = &mcp_binary {
        tracing::info!("MCP: using stdio binary at {}", mcp_bin.display());
        serde_json::json!([{
            "type": "stdio",
            "name": "rotero",
            "command": mcp_bin.to_string_lossy(),
            "args": [],
            "env": [{
                "name": "PDFIUM_DYNAMIC_LIB_PATH",
                "value": pdfium_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
            }]
        }])
    } else {
        tracing::warn!("MCP: no server available — agent won't have library tools");
        serde_json::json!([])
    }
}

pub(crate) fn is_auth_error(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("authentication required")
        || lower.contains("auth_required")
        || lower.contains("api key")
        || lower.contains("not configured")
        || lower.contains("not authenticated")
        || lower.contains("login required")
        || lower.contains("unauthorized")
        || lower.contains("credentials")
}

pub(crate) fn wait_for_switch_or_shutdown(req_rx: &mpsc::Receiver<ChatRequest>) -> LoopResult {
    loop {
        match req_rx.recv() {
            Ok(ChatRequest::SwitchAgent { provider_id }) => {
                return LoopResult::SwitchAgent(provider_id);
            }
            Ok(ChatRequest::Shutdown) => return LoopResult::Shutdown,
            Err(_) => return LoopResult::Shutdown,
            _ => {}
        }
    }
}

pub(crate) fn handle_notification(
    evt_tx: &tokio::sync::mpsc::UnboundedSender<ChatEvent>,
    v: &serde_json::Value,
) {
    let method = v.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "sessionUpdate" | "session/update" => {
            let params = match v.get("params") {
                Some(p) => p,
                None => return,
            };
            let update = match params.get("update") {
                Some(u) => u,
                None => return,
            };
            let update_type = update
                .get("sessionUpdate")
                .and_then(|u| u.as_str())
                .unwrap_or("");

            match update_type {
                "user_message_chunk" => {
                    if let Some(text) = update
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        let cleaned = strip_protocol_tags(text);
                        if !cleaned.is_empty() {
                            let _ = evt_tx.send(ChatEvent::UserMessage(cleaned));
                        }
                    }
                }
                "agent_message_chunk" => {
                    if let Some(text) = update
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        let cleaned = strip_protocol_tags(text);
                        if !cleaned.is_empty() {
                            let _ = evt_tx.send(ChatEvent::TextDelta(cleaned));
                        }
                    }
                }
                "tool_call" => {
                    let id = update
                        .get("toolCallId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let title = update
                        .get("title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let _ = evt_tx.send(ChatEvent::ToolCallStarted { id, title });
                }
                "tool_call_update" => {
                    let id = update
                        .get("toolCallId")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let status = match update.get("status").and_then(|s| s.as_str()) {
                        Some("pending") => ToolStatus::Pending,
                        Some("in_progress") => ToolStatus::InProgress,
                        Some("completed") => ToolStatus::Completed,
                        Some("failed") => ToolStatus::Failed,
                        _ => return,
                    };
                    let output = update
                        .get("content")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| {
                            let texts: Vec<String> = arr
                                .iter()
                                .filter_map(|item| {
                                    item.get("text")
                                        .and_then(|t| t.as_str())
                                        .map(String::from)
                                        .or_else(|| {
                                            item.get("content")
                                                .and_then(|c| c.get("text"))
                                                .and_then(|t| t.as_str())
                                                .map(String::from)
                                        })
                                })
                                .collect();
                            if texts.is_empty() { None } else { Some(texts.join("\n")) }
                        });
                    let _ = evt_tx.send(ChatEvent::ToolCallUpdated { id, status, output });
                }
                "available_commands_update" => {
                    let commands = update
                        .get("availableCommands")
                        .and_then(|c| c.as_array())
                        .map(|arr| {
                            arr.iter()
                                .map(|c| SlashCommand {
                                    name: c
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    description: c
                                        .get("description")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string(),
                                    hint: c
                                        .get("input")
                                        .and_then(|i| i.get("hint"))
                                        .and_then(|h| h.as_str())
                                        .map(String::from),
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = evt_tx.send(ChatEvent::CommandsAvailable(commands));
                }
                _ => {}
            }
        }
        "session/requestPermission" => {
            if let Some(id) = v.get("id") {
                tracing::debug!("ACP: auto-allowing permission request {id}");
            }
        }
        _ => {}
    }
}

pub(crate) fn api_key_env_for_method(method_id: &str) -> Option<String> {
    match method_id {
        "gemini-api-key" => Some("GEMINI_API_KEY".into()),
        "codex-api-key" | "openai-api-key" => Some("OPENAI_API_KEY".into()),
        "codex_api_key" => Some("CODEX_API_KEY".into()),
        id if id.contains("api-key") || id.contains("api_key") => {
            Some(id.to_uppercase().replace('-', "_"))
        }
        _ => None,
    }
}

pub(crate) fn extract_permission_options(v: &serde_json::Value) -> Vec<(String, String)> {
    v.get("params")
        .and_then(|p| p.get("options"))
        .and_then(|o| o.as_array())
        .map(|arr| {
            arr.iter()
                .map(|opt| {
                    let id = opt.get("optionId").and_then(|v| v.as_str()).unwrap_or("default").to_string();
                    let label = opt.get("label").and_then(|v| v.as_str())
                        .or_else(|| opt.get("name").and_then(|v| v.as_str()))
                        .unwrap_or(&id)
                        .to_string();
                    (id, label)
                })
                .collect()
        })
        .unwrap_or_else(|| vec![("default".into(), "Allow".into())])
}

pub(crate) fn first_allow_option_id(v: &serde_json::Value) -> String {
    v.get("params")
        .and_then(|p| p.get("options"))
        .and_then(|o| o.as_array())
        .and_then(|arr| {
            arr.iter()
                .find(|opt| {
                    let kind = opt.get("kind").and_then(|k| k.as_str()).unwrap_or("");
                    kind.contains("allow") || kind == "default"
                })
                .or_else(|| arr.first())
        })
        .and_then(|opt| opt.get("optionId").and_then(|id| id.as_str()))
        .unwrap_or("default")
        .to_string()
}

pub(crate) fn strip_protocol_tags(text: &str) -> String {
    let tag_patterns = [
        "command-name", "command-message", "command-args",
        "local-command-stdout", "local-command-stderr", "local-command-caveat",
        "system-reminder", "task-notification", "task-id", "tool-use-id",
        "output-file", "status", "summary",
        "rotero-context",
    ];

    let mut result = text.to_string();
    for tag in &tag_patterns {
        let open = format!("<{tag}");
        let close = format!("</{tag}>");
        while let Some(start) = result.find(&open) {
            if let Some(end) = result[start..].find(&close) {
                result = format!("{}{}", &result[..start], &result[start + end + close.len()..]);
            } else if let Some(end) = result[start..].find('>') {
                result = format!("{}{}", &result[..start], &result[start + end + 1..]);
            } else {
                break;
            }
        }
    }

    result.trim().to_string()
}

pub(crate) fn extract_models_event(models: &serde_json::Value) -> ChatEvent {
    let available = models
        .get("availableModels")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| AgentModel {
                    id: m.get("modelId").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    name: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    description: m.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    let current = models
        .get("currentModelId")
        .and_then(|v| v.as_str())
        .unwrap_or("default")
        .to_string();

    ChatEvent::ModelsAvailable { models: available, current }
}

pub(crate) fn extract_auth_methods(init_result: &serde_json::Value) -> Vec<AgentAuthMethod> {
    init_result
        .get("authMethods")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .map(|m| {
                    let (terminal_command, terminal_args) = m
                        .get("_meta")
                        .and_then(|meta| meta.get("terminal-auth"))
                        .map(|ta| {
                            let cmd = ta
                                .get("command")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let args: Vec<String> = ta
                                .get("args")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| v.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default();
                            (Some(cmd), args)
                        })
                        .unwrap_or((None, vec![]));

                    AgentAuthMethod {
                        id: m
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: m
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: m
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(String::from),
                        terminal_command,
                        terminal_args,
                        is_api_key: m
                            .get("_meta")
                            .and_then(|meta| meta.get("api-key"))
                            .is_some(),
                        api_key_env_var: api_key_env_for_method(
                            m.get("id").and_then(|v| v.as_str()).unwrap_or(""),
                        ),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

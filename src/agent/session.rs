use std::io::Write;
use std::sync::mpsc;

use super::LoopResult;
use super::connection::RawAcpConnection;
use super::helpers::{
    agent_working_dir, build_mcp_servers_json, extract_auth_methods, extract_models_event,
    extract_permission_options, first_allow_option_id, handle_notification, is_auth_error,
    wait_for_switch_or_shutdown,
};
use super::install::ensure_agent_installed;
use super::types::{AgentProvider, ChatEvent, ChatRequest, PastSession};

pub(crate) fn connect_and_run(
    provider: &AgentProvider,
    req_rx: &mpsc::Receiver<ChatRequest>,
    evt_tx: &tokio::sync::mpsc::UnboundedSender<ChatEvent>,
) -> LoopResult {
    tracing::info!(
        "ACP: connecting to {} ({})",
        provider.name,
        provider.npm_package
    );

    let entry_point = match ensure_agent_installed(provider) {
        Ok(ep) => ep,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "Failed to install {}: {e}",
                provider.name
            )));
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    let mut conn = match RawAcpConnection::spawn(&entry_point, provider.extra_args) {
        Ok(c) => c,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "Failed to start {}: {e}",
                provider.name
            )));
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    let init_params = serde_json::json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "_meta": { "terminal-auth": true }
        },
        "clientInfo": {
            "name": "rotero",
            "version": env!("CARGO_PKG_VERSION"),
            "title": "Rotero Paper Reader"
        }
    });

    let init_result = match conn.send_request("initialize", init_params, None) {
        Ok(r) => r,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "{} initialize failed: {e}",
                provider.name
            )));
            conn.kill();
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    tracing::info!("ACP: initialized {}", provider.name);

    let auth_methods = extract_auth_methods(&init_result);
    let supports_list = init_result
        .pointer("/agentCapabilities/sessionCapabilities/list")
        .is_some();
    let _ = evt_tx.send(ChatEvent::Connected {
        auth_methods,
        provider_id: provider.id.to_string(),
        supports_list_sessions: supports_list,
    });

    let mcp_servers = build_mcp_servers_json();

    let session_params = serde_json::json!({
        "cwd": agent_working_dir().to_string_lossy(),
        "mcpServers": mcp_servers,
    });

    let mut session_id = String::new();
    match conn.send_request("session/new", session_params, Some(evt_tx)) {
        Ok(r) => {
            session_id = r
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            tracing::info!("ACP: session created: {session_id}");
            let _ = evt_tx.send(ChatEvent::SessionCreated);

            if let Some(models) = r.get("models") {
                let _ = evt_tx.send(extract_models_event(models));
            }
        }
        Err(e) if is_auth_error(&e) => {
            let _ = evt_tx.send(ChatEvent::AuthRequired {
                provider_name: provider.name.to_string(),
            });
        }
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!("Failed to create session: {e}")));
            conn.kill();
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    while let Some(line) = conn.try_read_line() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            handle_notification(evt_tx, &v);
        }
    }

    let mut pending_auth_id: Option<u64> = None;
    let mut pending_auth_start: Option<std::time::Instant> = None;
    const AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
    let result = loop {
        match req_rx.try_recv() {
            Ok(ChatRequest::SendMessage {
                prompt,
                paper_context,
            }) => {
                let full_prompt = match paper_context {
                    Some(ctx) => format!("{ctx}\n\n{prompt}"),
                    None => prompt,
                };

                let prompt_params = serde_json::json!({
                    "sessionId": session_id,
                    "prompt": [{ "type": "text", "text": full_prompt }],
                });

                let prompt_id = conn.next_id;
                conn.next_id += 1;
                let msg = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": prompt_id,
                    "method": "session/prompt",
                    "params": prompt_params,
                });
                if let Some(stdin) = conn.child.stdin.as_mut() {
                    let line = serde_json::to_string(&msg).unwrap();
                    let _ = stdin.write_all(line.as_bytes());
                    let _ = stdin.write_all(b"\n");
                    let _ = stdin.flush();
                }

                loop {
                    match req_rx.try_recv() {
                        Ok(ChatRequest::PermissionResponse {
                            request_id,
                            option_id,
                        }) => {
                            let response = serde_json::json!({
                                "jsonrpc": "2.0",
                                "id": request_id,
                                "result": { "outcome": { "outcome": "selected", "optionId": option_id } }
                            });
                            let _ = conn.write_message(&response);
                        }
                        Ok(ChatRequest::Cancel) => {
                            let _ = conn.send_notification(
                                "session/cancel",
                                serde_json::json!({ "sessionId": session_id }),
                            );
                        }
                        _ => {}
                    }

                    match conn.incoming.try_recv() {
                        Err(mpsc::TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                            continue;
                        }
                        Err(mpsc::TryRecvError::Disconnected) => {
                            let _ = evt_tx.send(ChatEvent::Error("Connection closed".into()));
                            break;
                        }
                        Ok(line) => {
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                                if v.get("id").and_then(|i| i.as_u64()) == Some(prompt_id) {
                                    if v.get("error").is_some() {
                                        let _ = evt_tx.send(ChatEvent::Error(format!(
                                            "Prompt error: {}",
                                            v["error"]
                                        )));
                                    } else {
                                        let _ = evt_tx.send(ChatEvent::TurnCompleted);
                                    }
                                    break;
                                } else if v.get("method").and_then(|m| m.as_str())
                                    == Some("session/request_permission")
                                {
                                    if let Some(req_id) = v.get("id") {
                                        let tool_title = v
                                            .pointer("/params/toolCall/title")
                                            .and_then(|t| t.as_str())
                                            .unwrap_or("Tool call")
                                            .to_string();
                                        let options = extract_permission_options(&v);
                                        let _ = evt_tx.send(ChatEvent::PermissionRequest {
                                            request_id: req_id.clone(),
                                            tool_title,
                                            options,
                                        });
                                    }
                                } else {
                                    let method =
                                        v.get("method").and_then(|m| m.as_str()).unwrap_or("");
                                    let has_id = v.get("id").is_some();
                                    if !has_id
                                        || method == "session/update"
                                        || method == "sessionUpdate"
                                    {
                                        handle_notification(evt_tx, &v);
                                    } else {
                                        tracing::warn!("ACP: unhandled agent request: {method}");
                                        let response = serde_json::json!({
                                            "jsonrpc": "2.0",
                                            "id": v.get("id"),
                                            "error": { "code": -32601, "message": "Method not found" }
                                        });
                                        let _ = conn.write_message(&response);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Ok(ChatRequest::Cancel) => {
                let _ = conn.send_notification(
                    "session/cancel",
                    serde_json::json!({ "sessionId": session_id }),
                );
            }
            Ok(ChatRequest::PermissionResponse {
                request_id,
                option_id,
            }) => {
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "result": { "outcome": { "outcome": "selected", "optionId": option_id } }
                });
                let _ = conn.write_message(&response);
            }
            Ok(ChatRequest::ListSessions) => {
                match conn.send_request(
                    "session/list",
                    serde_json::json!({
                        "cwd": agent_working_dir().to_string_lossy(),
                    }),
                    None,
                ) {
                    Ok(result) => {
                        let sessions = result
                            .get("sessions")
                            .and_then(|s| s.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .map(|s| PastSession {
                                        session_id: s
                                            .get("sessionId")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        cwd: s
                                            .get("cwd")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        title: s
                                            .get("title")
                                            .and_then(|v| v.as_str())
                                            .map(String::from),
                                        updated_at: s
                                            .get("updatedAt")
                                            .and_then(|v| v.as_str())
                                            .map(String::from),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let _ = evt_tx.send(ChatEvent::SessionList(sessions));
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("List sessions failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::LoadSession {
                session_id: load_id,
                cwd,
            }) => {
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider.id.to_string(),
                });
                let load_cwd = if cwd.is_empty() {
                    agent_working_dir().to_string_lossy().to_string()
                } else {
                    cwd
                };
                let params = serde_json::json!({
                    "sessionId": load_id,
                    "cwd": load_cwd,
                    "mcpServers": build_mcp_servers_json(),
                });
                match conn.send_request("session/load", params, Some(evt_tx)) {
                    Ok(result) => {
                        if let Some(sid) = result.get("sessionId").and_then(|v| v.as_str()) {
                            session_id = sid.to_string();
                        }
                        let _ = evt_tx.send(ChatEvent::SessionCreated);
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("Load session failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::SwitchAgent { provider_id }) => {
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider_id.clone(),
                });
                break LoopResult::SwitchAgent(provider_id);
            }
            Ok(ChatRequest::SetModel { model_id }) => {
                let params = serde_json::json!({
                    "sessionId": session_id,
                    "modelId": model_id,
                });
                match conn.send_request("session/set_model", params, None) {
                    Ok(_) => {
                        tracing::info!("ACP: model set to {model_id}");
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("Set model failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::Authenticate { method_id }) => {
                // Auth response may take a long time (browser OAuth flow);
                // handle it in the idle loop below.
                let auth_id = conn.next_id;
                conn.next_id += 1;
                let msg = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": auth_id,
                    "method": "authenticate",
                    "params": { "methodId": method_id },
                });
                if let Err(e) = conn.write_message(&msg) {
                    let _ = evt_tx.send(ChatEvent::Error(format!("Auth send failed: {e}")));
                } else {
                    let _ = evt_tx.send(ChatEvent::Switching {
                        provider_id: provider.id.to_string(),
                    });
                    pending_auth_id = Some(auth_id);
                    pending_auth_start = Some(std::time::Instant::now());
                }
            }
            Ok(ChatRequest::Shutdown) => {
                break LoopResult::Shutdown;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                break LoopResult::Shutdown;
            }
            Err(mpsc::TryRecvError::Empty) => {
                while let Some(line) = conn.try_read_line() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                        if let Some(auth_id) = pending_auth_id
                            && v.get("id").and_then(|i| i.as_u64()) == Some(auth_id)
                        {
                            pending_auth_id = None;
                            if v.get("error").is_some() {
                                let _ = evt_tx
                                    .send(ChatEvent::Error(format!("Auth failed: {}", v["error"])));
                            } else {
                                tracing::info!("ACP: auth completed, creating session...");
                                let session_params = serde_json::json!({
                                    "cwd": agent_working_dir().to_string_lossy(),
                                    "mcpServers": build_mcp_servers_json(),
                                });
                                match conn.send_request("session/new", session_params, Some(evt_tx))
                                {
                                    Ok(r) => {
                                        if let Some(sid) =
                                            r.get("sessionId").and_then(|v| v.as_str())
                                        {
                                            session_id = sid.to_string();
                                        }
                                        let _ = evt_tx.send(ChatEvent::SessionCreated);
                                    }
                                    Err(e) if is_auth_error(&e) => {
                                        let _ = evt_tx.send(ChatEvent::AuthRequired {
                                            provider_name: provider.name.to_string(),
                                        });
                                    }
                                    Err(e) => {
                                        let _ = evt_tx
                                            .send(ChatEvent::Error(format!("Session failed: {e}")));
                                    }
                                }
                            }
                            continue;
                        }
                        if v.get("method").and_then(|m| m.as_str())
                            == Some("session/request_permission")
                        {
                            if let Some(req_id) = v.get("id") {
                                let option_id = first_allow_option_id(&v);
                                let response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": req_id,
                                    "result": { "outcome": { "outcome": "selected", "optionId": option_id } }
                                });
                                let _ = conn.write_message(&response);
                            }
                            continue;
                        }
                        handle_notification(evt_tx, &v);
                    }
                }
                if let (Some(_), Some(start)) = (pending_auth_id, pending_auth_start)
                    && start.elapsed() > AUTH_TIMEOUT
                {
                    pending_auth_id = None;
                    pending_auth_start = None;
                    let _ = evt_tx.send(ChatEvent::Error(
                        "Sign in timed out. Try again from Settings > AI Agent.".into(),
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    };

    conn.kill();
    result
}

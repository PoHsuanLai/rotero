pub mod types;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

use types::{
    AgentAuthMethod, AgentProvider, ChatEvent, ChatRequest, PastSession, SlashCommand, ToolStatus,
    AGENT_PROVIDERS,
};

fn find_mcp_binary() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("rotero-mcp");
        if sibling.exists() {
            return Some(sibling);
        }
    }
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release/rotero-mcp");
    if dev.exists() {
        return Some(dev);
    }
    let debug = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/rotero-mcp");
    if debug.exists() {
        return Some(debug);
    }
    which::which("rotero-mcp").ok()
}

fn find_pdfium_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
        return Some(PathBuf::from(p));
    }
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib");
    if lib.exists() {
        return Some(lib);
    }
    None
}

fn agents_cache_dir() -> PathBuf {
    let base = directories::BaseDirs::new()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.rotero.Rotero").join("agents")
}

fn ensure_agent_installed(provider: &AgentProvider) -> Result<PathBuf, String> {
    let cache = agents_cache_dir();
    let pkg_dir = cache.join(provider.id);
    let pkg_root = pkg_dir.join("node_modules").join(provider.npm_package);
    let pkg_json_path = pkg_root.join("package.json");

    if pkg_json_path.exists() {
        return resolve_bin_entry(&pkg_root);
    }

    std::fs::create_dir_all(&pkg_dir)
        .map_err(|e| format!("Failed to create agent cache dir: {e}"))?;

    tracing::info!("Installing {} (first time setup)...", provider.npm_package);

    let output = Command::new("npm")
        .args([
            "install",
            "--prefix",
            &pkg_dir.to_string_lossy(),
            provider.npm_package,
        ])
        .output()
        .map_err(|e| format!("Failed to run npm install: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("npm install failed: {stderr}"));
    }

    resolve_bin_entry(&pkg_root)
}

fn resolve_bin_entry(pkg_root: &PathBuf) -> Result<PathBuf, String> {
    let pkg_json = pkg_root.join("package.json");
    let content =
        std::fs::read_to_string(&pkg_json).map_err(|e| format!("Can't read package.json: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid package.json: {e}"))?;

    let bin_path = match v.get("bin") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(obj)) => obj
            .values()
            .next()
            .and_then(|v| v.as_str())
            .ok_or("No bin entries in package.json")?
            .to_string(),
        _ => return Err("No bin field in package.json".into()),
    };

    let entry = pkg_root.join(&bin_path);
    if entry.exists() {
        Ok(entry)
    } else {
        Err(format!("Entry point not found: {}", entry.display()))
    }
}

pub fn spawn_agent_thread() -> (
    mpsc::Sender<ChatRequest>,
    tokio::sync::mpsc::UnboundedReceiver<ChatEvent>,
) {
    let (req_tx, req_rx) = mpsc::channel::<ChatRequest>();
    let (evt_tx, evt_rx) = tokio::sync::mpsc::unbounded_channel::<ChatEvent>();

    std::thread::Builder::new()
        .name("acp-agent".into())
        .spawn(move || agent_main(req_rx, evt_tx))
        .expect("Failed to spawn ACP agent thread");

    (req_tx, evt_rx)
}

fn agent_main(
    req_rx: mpsc::Receiver<ChatRequest>,
    evt_tx: tokio::sync::mpsc::UnboundedSender<ChatEvent>,
) {
    let config = crate::sync::engine::SyncConfig::load();
    let mut current_provider = AGENT_PROVIDERS
        .iter()
        .find(|p| p.id == config.agent_provider)
        .unwrap_or(&AGENT_PROVIDERS[0]);

    loop {
        let result = connect_and_run(current_provider, &req_rx, &evt_tx);

        match result {
            LoopResult::SwitchAgent(provider_id) => {
                if let Some(provider) = AGENT_PROVIDERS.iter().find(|p| p.id == provider_id) {
                    current_provider = provider;
                    continue;
                } else {
                    let _ = evt_tx.send(ChatEvent::Error(format!(
                        "Unknown agent provider: {provider_id}"
                    )));
                    break;
                }
            }
            LoopResult::Shutdown => break,
        }
    }
}

enum LoopResult {
    SwitchAgent(String),
    Shutdown,
}

/// A raw JSON-RPC connection over stdio to an ACP agent subprocess.
/// Uses a background reader thread to avoid blocking on pipe reads.
struct RawAcpConnection {
    child: Child,
    /// Lines received from the agent subprocess (non-blocking).
    incoming: mpsc::Receiver<String>,
    next_id: u64,
}

impl RawAcpConnection {
    fn spawn(entry_point: &PathBuf, extra_args: &[&str]) -> Result<Self, String> {
        let mut cmd = Command::new("node");
        cmd.arg(entry_point);
        for arg in extra_args {
            cmd.arg(arg);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn node: {e}"))?;
        let stdout = child.stdout.take().ok_or("No stdout")?;

        // Spawn a background thread to read lines from stdout without blocking
        let (line_tx, line_rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("acp-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            if line_tx.send(line).is_err() {
                                break; // receiver dropped
                            }
                        }
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn reader thread: {e}"))?;

        Ok(Self {
            child,
            incoming: line_rx,
            next_id: 1,
        })
    }

    fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.write_message(&msg)?;

        // Read lines until we get the response with matching id.
        // Auto-respond to requestPermission to avoid deadlocks.
        loop {
            let line = self
                .incoming
                .recv()
                .map_err(|_| "Connection closed".to_string())?;

            let v: serde_json::Value =
                serde_json::from_str(line.trim()).map_err(|e| format!("Parse error: {e}"))?;

            // Check if this is our response
            if v.get("id").and_then(|i| i.as_u64()) == Some(id) {
                if let Some(error) = v.get("error") {
                    return Err(format!("RPC error: {error}"));
                }
                return v
                    .get("result")
                    .cloned()
                    .ok_or("No result in response".into());
            }

            // Auto-respond to requestPermission requests from the agent
            if v.get("method").and_then(|m| m.as_str()) == Some("session/requestPermission") {
                if let Some(req_id) = v.get("id") {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": { "outcome": { "type": "selected", "optionId": "allow" } }
                    });
                    let _ = self.write_message(&response);
                }
            }

            // Other messages are notifications — skip
        }
    }

    fn write_message(&mut self, msg: &serde_json::Value) -> Result<(), String> {
        let stdin = self.child.stdin.as_mut().ok_or("No stdin")?;
        let line = serde_json::to_string(msg).map_err(|e| format!("JSON error: {e}"))?;
        stdin
            .write_all(line.as_bytes())
            .map_err(|e| format!("Write error: {e}"))?;
        stdin
            .write_all(b"\n")
            .map_err(|e| format!("Write error: {e}"))?;
        stdin.flush().map_err(|e| format!("Flush error: {e}"))?;
        Ok(())
    }

    fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), String> {
        let msg = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&msg)
    }

    /// Non-blocking read of a line from the agent.
    fn try_read_line(&mut self) -> Option<String> {
        self.incoming.try_recv().ok()
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connect_and_run(
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

    // Initialize
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

    let init_result = match conn.send_request("initialize", init_params) {
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
    let _ = evt_tx.send(ChatEvent::Connected {
        auth_methods,
        provider_id: provider.id.to_string(),
    });

    // Create session with MCP
    let mcp_servers = build_mcp_servers_json();

    let session_params = serde_json::json!({
        "cwd": home_dir_or_cwd().to_string_lossy(),
        "mcpServers": mcp_servers,
    });

    let session_result = match conn.send_request("session/new", session_params) {
        Ok(r) => r,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!("Failed to create session: {e}")));
            conn.kill();
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    let mut session_id = session_result
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    tracing::info!("ACP: session created: {session_id}");
    let _ = evt_tx.send(ChatEvent::SessionCreated);

    // Drain any notifications that arrived during init/session setup
    while let Some(line) = conn.try_read_line() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            handle_notification(evt_tx, &v);
        }
    }

    // Main message loop
    let result = loop {
        // Check for UI requests
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

                // Send prompt request (non-blocking write)
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

                // Read responses until we get the prompt result
                loop {
                    match conn.incoming.recv() {
                        Err(_) => {
                            let _ = evt_tx.send(ChatEvent::Error("Connection closed".into()));
                            break;
                        }
                        Ok(line) => {
                            if let Ok(v) =
                                serde_json::from_str::<serde_json::Value>(line.trim())
                            {
                                if v.get("id").and_then(|i| i.as_u64()) == Some(prompt_id) {
                                    // Prompt completed
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
                                    == Some("session/requestPermission")
                                {
                                    // Auto-allow permission requests
                                    if let Some(req_id) = v.get("id") {
                                        let response = serde_json::json!({
                                            "jsonrpc": "2.0",
                                            "id": req_id,
                                            "result": { "outcome": { "type": "selected", "optionId": "allow" } }
                                        });
                                        let _ = conn.write_message(&response);
                                    }
                                } else {
                                    handle_notification(evt_tx, &v);
                                }
                            }
                        }
                        Err(e) => {
                            let _ = evt_tx
                                .send(ChatEvent::Error(format!("Read error: {e}")));
                            break;
                        }
                    }

                    // Check for cancel requests while streaming
                    if let Ok(ChatRequest::Cancel) = req_rx.try_recv() {
                        let _ = conn.send_notification(
                            "session/cancel",
                            serde_json::json!({ "sessionId": session_id }),
                        );
                    }
                }
            }
            Ok(ChatRequest::Cancel) => {
                let _ = conn.send_notification(
                    "session/cancel",
                    serde_json::json!({ "sessionId": session_id }),
                );
            }
            Ok(ChatRequest::ListSessions) => {
                match conn.send_request("session/list", serde_json::json!({})) {
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
                        let _ = evt_tx
                            .send(ChatEvent::Error(format!("List sessions failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::LoadSession {
                session_id: load_id,
            }) => {
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider.id.to_string(),
                });
                let params = serde_json::json!({
                    "sessionId": load_id,
                    "cwd": home_dir_or_cwd().to_string_lossy(),
                    "mcpServers": build_mcp_servers_json(),
                });
                match conn.send_request("session/load", params) {
                    Ok(result) => {
                        // Update session_id to the loaded one
                        if let Some(sid) = result.get("sessionId").and_then(|v| v.as_str()) {
                            session_id = sid.to_string();
                        }
                        let _ = evt_tx.send(ChatEvent::SessionCreated);
                    }
                    Err(e) => {
                        let _ = evt_tx
                            .send(ChatEvent::Error(format!("Load session failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::SwitchAgent { provider_id }) => {
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider_id.clone(),
                });
                break LoopResult::SwitchAgent(provider_id);
            }
            Ok(ChatRequest::Authenticate { .. }) => {
                // Auth is handled by spawning terminal commands, not through RPC
            }
            Ok(ChatRequest::Shutdown) => {
                break LoopResult::Shutdown;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                break LoopResult::Shutdown;
            }
            Err(mpsc::TryRecvError::Empty) => {
                // Drain any async notifications
                while let Some(line) = conn.try_read_line() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                        handle_notification(evt_tx, &v);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    };

    conn.kill();
    result
}

/// Get the user's home directory, falling back to cwd.
fn home_dir_or_cwd() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// Build the JSON array of MCP servers to attach to any session.
fn build_mcp_servers_json() -> serde_json::Value {
    let mcp_binary = find_mcp_binary();
    let pdfium_path = find_pdfium_path();

    if let Some(mcp_bin) = &mcp_binary {
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
        serde_json::json!([])
    }
}

fn wait_for_switch_or_shutdown(req_rx: &mpsc::Receiver<ChatRequest>) -> LoopResult {
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

fn handle_notification(
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
                "agent_message_chunk" => {
                    if let Some(text) = update
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        let _ = evt_tx.send(ChatEvent::TextDelta(text.to_string()));
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
                    let _ = evt_tx.send(ChatEvent::ToolCallUpdated { id, status });
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
            // Auto-allow all tool calls for now
            // The agent sends this as a request (has an id), we need to respond
            if let Some(id) = v.get("id") {
                tracing::debug!("ACP: auto-allowing permission request {id}");
                // We can't respond here easily since we don't have &mut conn
                // TODO: handle permission requests properly
            }
        }
        _ => {}
    }
}

fn extract_auth_methods(init_result: &serde_json::Value) -> Vec<AgentAuthMethod> {
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
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

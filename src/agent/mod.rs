pub mod types;

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;

use types::{
    AgentAuthMethod, AgentModel, AgentProvider, ChatEvent, ChatRequest, PastSession, SlashCommand,
    ToolStatus, AGENT_PROVIDERS,
};

fn find_mcp_binary() -> Option<PathBuf> {
    // 1. Next to the running binary (production)
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("rotero-mcp");
        if sibling.exists() {
            return Some(sibling);
        }
    }

    // 2. In the workspace target dir (development — handles worktrees)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in [&manifest_dir, &manifest_dir.join(".."), &manifest_dir.join("../..")]  {
        for profile in ["release", "debug"] {
            let candidate = dir.join("target").join(profile).join("rotero-mcp");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 3. In PATH
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

/// The Node.js version to download if not found on system.
const NODE_VERSION: &str = "v22.16.0";

/// Find node in PATH, or download and cache it.
fn find_or_install_node() -> Result<PathBuf, String> {
    // 1. Check PATH
    if let Ok(path) = which::which("node") {
        return Ok(path);
    }

    // 2. Check our local cache
    let node_dir = node_cache_dir();
    let node_bin = node_binary_path(&node_dir);
    if node_bin.exists() {
        return Ok(node_bin);
    }

    // 3. Download
    tracing::info!("Node.js not found, downloading {NODE_VERSION}...");
    download_node(&node_dir)?;

    if node_bin.exists() {
        Ok(node_bin)
    } else {
        Err("Downloaded Node.js but binary not found".into())
    }
}

/// Also need npm for installing agent packages.
fn find_npm() -> Result<PathBuf, String> {
    if let Ok(path) = which::which("npm") {
        return Ok(path);
    }
    let node_dir = node_cache_dir();
    let npm_bin = npm_binary_path(&node_dir);
    if npm_bin.exists() {
        return Ok(npm_bin);
    }
    Err("npm not found. Install Node.js or use the AI Agent settings to auto-download.".into())
}

fn node_cache_dir() -> PathBuf {
    let base = directories::BaseDirs::new()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.rotero.Rotero").join("node")
}

fn node_binary_path(node_dir: &PathBuf) -> PathBuf {
    if cfg!(windows) {
        node_dir.join("node.exe")
    } else {
        node_dir.join("bin").join("node")
    }
}

fn npm_binary_path(node_dir: &PathBuf) -> PathBuf {
    if cfg!(windows) {
        node_dir.join("npm.cmd")
    } else {
        node_dir.join("bin").join("npm")
    }
}

fn download_node(node_dir: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(node_dir)
        .map_err(|e| format!("Failed to create node cache dir: {e}"))?;

    let (os, arch, ext) = node_platform();
    let filename = format!("node-{NODE_VERSION}-{os}-{arch}");
    let archive_name = format!("{filename}.{ext}");
    let url = format!("https://nodejs.org/dist/{NODE_VERSION}/{archive_name}");

    tracing::info!("Downloading Node.js from {url}");

    // Download to temp file
    let tmp_archive = node_dir.join(&archive_name);
    let output = Command::new("curl")
        .args(["-fsSL", "-o", &tmp_archive.to_string_lossy(), &url])
        .output()
        .map_err(|e| format!("Failed to download Node.js: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Download failed: {stderr}"));
    }

    // Extract
    if ext == "zip" {
        // Windows: unzip
        let output = Command::new("tar")
            .args(["-xf", &tmp_archive.to_string_lossy(), "-C", &node_dir.to_string_lossy()])
            .output()
            .map_err(|e| format!("Failed to extract: {e}"))?;
        if !output.status.success() {
            return Err("Failed to extract Node.js archive".into());
        }
    } else {
        // Unix: tar.xz
        let output = Command::new("tar")
            .args([
                "-xf",
                &tmp_archive.to_string_lossy(),
                "-C",
                &node_dir.to_string_lossy(),
                "--strip-components=1",
            ])
            .output()
            .map_err(|e| format!("Failed to extract: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to extract Node.js: {stderr}"));
        }
    }

    // Clean up archive
    let _ = std::fs::remove_file(&tmp_archive);

    tracing::info!("Node.js {NODE_VERSION} installed to {}", node_dir.display());
    Ok(())
}

fn node_platform() -> (&'static str, &'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "win"
    } else {
        "linux" // fallback
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "armv7l"
    } else {
        "x64" // fallback
    };

    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.xz"
    };

    (os, arch, ext)
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

    let npm_bin = find_npm()?;
    let output = Command::new(&npm_bin)
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

impl Drop for RawAcpConnection {
    fn drop(&mut self) {
        self.kill();
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
        let node_bin = find_or_install_node()?;
        let mut cmd = Command::new(&node_bin);
        cmd.arg(entry_point);
        for arg in extra_args {
            cmd.arg(arg);
        }
        // Pass any stored API keys as env vars
        let config = crate::sync::engine::SyncConfig::load();
        for (key, val) in &config.agent_api_keys {
            if !val.is_empty() {
                cmd.env(key, val);
            }
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Create new process group so we can kill all children on cleanup
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

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
        evt_tx: Option<&tokio::sync::mpsc::UnboundedSender<ChatEvent>>,
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

            // Auto-respond to requestPermission
            if v.get("method").and_then(|m| m.as_str()) == Some("session/requestPermission") {
                if let Some(req_id) = v.get("id") {
                    let response = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": { "outcome": { "type": "selected", "optionId": "allow" } }
                    });
                    let _ = self.write_message(&response);
                }
                continue;
            }

            // Forward notifications to event handler if available
            if let Some(tx) = evt_tx {
                handle_notification(tx, &v);
            }
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
        // Kill the entire process group to clean up node + any children
        #[cfg(unix)]
        {
            let pid = self.child.id();
            unsafe {
                // Kill process group (negative pid)
                libc::kill(-(pid as i32), libc::SIGTERM);
            }
            // Give it a moment to exit gracefully
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(not(unix))]
        {
            let _ = self.child.kill();
        }
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

    // Create session with MCP
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

            // Extract available models
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

    // Drain any notifications that arrived during init/session setup
    while let Some(line) = conn.try_read_line() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            handle_notification(evt_tx, &v);
        }
    }

    // Main message loop
    let mut pending_auth_id: Option<u64> = None;
    let mut pending_auth_start: Option<std::time::Instant> = None;
    const AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
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
                match conn.send_request("session/list", serde_json::json!({
                    "cwd": agent_working_dir().to_string_lossy(),
                }), None) {
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
                        let _ = evt_tx
                            .send(ChatEvent::Error(format!("List sessions failed: {e}")));
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
                // Send authenticate request non-blocking — the response
                // may take a long time (browser OAuth flow). We send the
                // request and handle the response in the idle loop.
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
                    // Track that we're waiting for an auth response
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
                // Drain any async notifications / pending auth responses
                while let Some(line) = conn.try_read_line() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(line.trim()) {
                        // Check if this is a pending auth response
                        if let Some(auth_id) = pending_auth_id {
                            if v.get("id").and_then(|i| i.as_u64()) == Some(auth_id) {
                                pending_auth_id = None;
                                if v.get("error").is_some() {
                                    let _ = evt_tx.send(ChatEvent::Error(format!(
                                        "Auth failed: {}",
                                        v["error"]
                                    )));
                                } else {
                                    tracing::info!("ACP: auth completed, creating session...");
                                    // Retry session creation after auth
                                    let session_params = serde_json::json!({
                                        "cwd": agent_working_dir().to_string_lossy(),
                                        "mcpServers": build_mcp_servers_json(),
                                    });
                                    match conn.send_request("session/new", session_params, Some(evt_tx)) {
                                        Ok(r) => {
                                            if let Some(sid) = r.get("sessionId").and_then(|v| v.as_str()) {
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
                                            let _ = evt_tx.send(ChatEvent::Error(format!("Session failed: {e}")));
                                        }
                                    }
                                }
                                continue;
                            }
                        }
                        // Handle requestPermission during auth
                        if v.get("method").and_then(|m| m.as_str())
                            == Some("session/requestPermission")
                        {
                            if let Some(req_id) = v.get("id") {
                                let response = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": req_id,
                                    "result": { "outcome": { "type": "selected", "optionId": "allow" } }
                                });
                                let _ = conn.write_message(&response);
                            }
                            continue;
                        }
                        handle_notification(evt_tx, &v);
                    }
                }
                // Check for auth timeout
                if let (Some(_), Some(start)) = (pending_auth_id, pending_auth_start) {
                    if start.elapsed() > AUTH_TIMEOUT {
                        pending_auth_id = None;
                        pending_auth_start = None;
                        let _ = evt_tx.send(ChatEvent::Error(
                            "Sign in timed out. Try again from Settings > AI Agent.".into(),
                        ));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    };

    conn.kill();
    result
}

/// Get the app's data directory as the agent working directory.
/// This is where papers and the database live.
fn agent_working_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.data_dir().join("com.rotero.Rotero"))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// Build the JSON array of MCP servers to attach to any session.
fn build_mcp_servers_json() -> serde_json::Value {
    let mcp_binary = find_mcp_binary();
    let pdfium_path = find_pdfium_path();

    match &mcp_binary {
        Some(p) => tracing::info!("MCP binary found: {}", p.display()),
        None => tracing::warn!("rotero-mcp binary not found — agent won't have library tools"),
    }

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

/// Check if an RPC error indicates authentication is needed.
fn is_auth_error(err: &str) -> bool {
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
                "user_message_chunk" => {
                    if let Some(text) = update
                        .get("content")
                        .and_then(|c| c.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        let _ = evt_tx.send(ChatEvent::UserMessage(text.to_string()));
                    }
                }
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
                    // Extract text output from content array
                    let output = update
                        .get("content")
                        .and_then(|c| c.as_array())
                        .and_then(|arr| {
                            let texts: Vec<String> = arr
                                .iter()
                                .filter_map(|item| {
                                    // Content blocks or text items
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

/// Map known auth method IDs to env var names.
fn api_key_env_for_method(method_id: &str) -> Option<String> {
    match method_id {
        "gemini-api-key" => Some("GEMINI_API_KEY".into()),
        "codex-api-key" | "openai-api-key" => Some("OPENAI_API_KEY".into()),
        "codex_api_key" => Some("CODEX_API_KEY".into()),
        id if id.contains("api-key") || id.contains("api_key") => {
            // Best-effort: uppercase the ID and replace dashes
            Some(id.to_uppercase().replace('-', "_"))
        }
        _ => None,
    }
}

fn extract_models_event(models: &serde_json::Value) -> ChatEvent {
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

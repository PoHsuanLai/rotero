pub mod types;

use std::path::PathBuf;
use std::sync::mpsc;

use acpx::AgentServer;
use agent_client_protocol as acp;
use futures::StreamExt as _;

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

fn build_agent_command(provider: &AgentProvider) -> acpx::CommandSpec {
    let mut spec = acpx::CommandSpec::new(provider.program);
    for arg in provider.args {
        spec = spec.arg(*arg);
    }
    spec
}

pub fn spawn_agent_thread() -> (
    mpsc::Sender<ChatRequest>,
    tokio::sync::mpsc::UnboundedReceiver<ChatEvent>,
) {
    let (req_tx, req_rx) = mpsc::channel::<ChatRequest>();
    let (evt_tx, evt_rx) = tokio::sync::mpsc::unbounded_channel::<ChatEvent>();

    std::thread::Builder::new()
        .name("acp-agent".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for ACP agent");

            let local = tokio::task::LocalSet::new();
            local.block_on(&rt, agent_main(req_rx, evt_tx));
        })
        .expect("Failed to spawn ACP agent thread");

    (req_tx, evt_rx)
}

async fn agent_main(
    req_rx: mpsc::Receiver<ChatRequest>,
    evt_tx: tokio::sync::mpsc::UnboundedSender<ChatEvent>,
) {
    let mcp_binary = find_mcp_binary();
    let pdfium_path = find_pdfium_path();

    let acp_runtime = acpx::RuntimeContext::new(|task| {
        tokio::task::spawn_local(task);
    });

    // Start with the provider from config
    let config = crate::sync::engine::SyncConfig::load();
    let mut current_provider = AGENT_PROVIDERS
        .iter()
        .find(|p| p.id == config.agent_provider)
        .unwrap_or(&AGENT_PROVIDERS[0]);

    loop {
        // Connect to the current provider
        let result = connect_and_run(
            current_provider,
            &acp_runtime,
            &mcp_binary,
            &pdfium_path,
            &req_rx,
            &evt_tx,
        )
        .await;

        match result {
            AgentLoopResult::SwitchAgent(provider_id) => {
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
            AgentLoopResult::Shutdown => break,
        }
    }
}

enum AgentLoopResult {
    SwitchAgent(String),
    Shutdown,
}

async fn connect_and_run(
    provider: &AgentProvider,
    acp_runtime: &acpx::RuntimeContext,
    mcp_binary: &Option<PathBuf>,
    pdfium_path: &Option<PathBuf>,
    req_rx: &mpsc::Receiver<ChatRequest>,
    evt_tx: &tokio::sync::mpsc::UnboundedSender<ChatEvent>,
) -> AgentLoopResult {
    let agent_cmd = build_agent_command(provider);

    let server = acpx::CommandAgentServer::new(
        acpx::AgentServerMetadata::new(provider.id, provider.name, "0.1.0"),
        agent_cmd,
    );

    let connection: acpx::Connection = match server.connect(acp_runtime).await {
        Ok(conn) => conn,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "Failed to connect to {}: {e}",
                provider.name
            )));
            // Wait for a SwitchAgent or Shutdown
            return wait_for_switch_or_shutdown(req_rx).await;
        }
    };

    // Declare terminal-auth support so adapter returns auth methods
    let mut meta = serde_json::Map::new();
    meta.insert("terminal-auth".into(), serde_json::Value::Bool(true));
    let capabilities = acp::ClientCapabilities::new().meta(acp::Meta::from(meta));

    let init_resp = match connection
        .initialize(
            acp::InitializeRequest::new(acp::ProtocolVersion::V1)
                .client_info(
                    acp::Implementation::new("rotero", env!("CARGO_PKG_VERSION"))
                        .title("Rotero Paper Reader"),
                )
                .client_capabilities(capabilities),
        )
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "{} initialize failed: {e}",
                provider.name
            )));
            let _ = connection.close().await;
            return wait_for_switch_or_shutdown(req_rx).await;
        }
    };

    let auth_methods = extract_auth_methods(&init_resp);

    let _ = evt_tx.send(ChatEvent::Connected {
        auth_methods,
        provider_id: provider.id.to_string(),
    });

    // Create session with MCP server
    let session_id = match create_session(&connection, mcp_binary, pdfium_path).await {
        Ok(id) => id,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(e));
            let _ = connection.close().await;
            return wait_for_switch_or_shutdown(req_rx).await;
        }
    };

    let _ = evt_tx.send(ChatEvent::SessionCreated);

    let mut updates = connection.subscribe_session_updates();
    let mut current_session_id = session_id;

    // Main message loop
    let result = loop {
        let req = loop {
            while let Ok(Some(n)) = updates.try_next() {
                handle_session_update(evt_tx, &n.update);
            }

            match req_rx.try_recv() {
                Ok(req) => break Some(req),
                Err(mpsc::TryRecvError::Disconnected) => break None,
                Err(mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            }
        };

        match req {
            Some(ChatRequest::SendMessage {
                prompt,
                paper_context,
            }) => {
                let full_prompt = match paper_context {
                    Some(ctx) => format!("{ctx}\n\n{prompt}"),
                    None => prompt,
                };

                let prompt_req = acp::PromptRequest::new(
                    current_session_id.clone(),
                    vec![full_prompt.into()],
                );

                let mut prompt_fut = std::pin::pin!(connection.prompt(prompt_req));
                let prompt_result = loop {
                    tokio::select! {
                        result = &mut prompt_fut => break result,
                        update = updates.next() => {
                            if let Some(n) = update {
                                handle_session_update(evt_tx, &n.update);
                            }
                        }
                    }
                };

                while let Ok(Some(n)) = updates.try_next() {
                    handle_session_update(evt_tx, &n.update);
                }

                match prompt_result {
                    Ok(_) => {
                        let _ = evt_tx.send(ChatEvent::TurnCompleted);
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("Prompt failed: {e}")));
                    }
                }
            }
            Some(ChatRequest::Cancel) => {
                let _ = connection
                    .cancel(acp::CancelNotification::new(current_session_id.clone()))
                    .await;
            }
            Some(ChatRequest::Authenticate { method_id }) => {
                let mid: acp::AuthMethodId = method_id.into();
                match connection
                    .authenticate(acp::AuthenticateRequest::new(mid))
                    .await
                {
                    Ok(_) => {
                        let _ = evt_tx.send(ChatEvent::SessionCreated);
                    }
                    Err(e) => {
                        let _ = evt_tx
                            .send(ChatEvent::Error(format!("Authentication failed: {e}")));
                    }
                }
            }
            Some(ChatRequest::ListSessions) => {
                match connection
                    .list_sessions(acp::ListSessionsRequest::default())
                    .await
                {
                    Ok(resp) => {
                        let sessions = resp
                            .sessions
                            .into_iter()
                            .map(|s| PastSession {
                                session_id: s.session_id.to_string(),
                                title: s.title,
                                updated_at: s.updated_at,
                            })
                            .collect();
                        let _ = evt_tx.send(ChatEvent::SessionList(sessions));
                    }
                    Err(e) => {
                        let _ = evt_tx
                            .send(ChatEvent::Error(format!("Failed to list sessions: {e}")));
                    }
                }
            }
            Some(ChatRequest::LoadSession { session_id }) => {
                let sid: acp::SessionId = session_id.into();
                match connection
                    .load_session(acp::LoadSessionRequest::new(
                        sid.clone(),
                        std::env::current_dir().unwrap_or_default(),
                    ))
                    .await
                {
                    Ok(_) => {
                        current_session_id = sid;
                        let _ = evt_tx.send(ChatEvent::SessionCreated);
                    }
                    Err(e) => {
                        let _ = evt_tx
                            .send(ChatEvent::Error(format!("Failed to load session: {e}")));
                    }
                }
            }
            Some(ChatRequest::SwitchAgent { provider_id }) => {
                // Send Switching event immediately for instant UI feedback
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider_id.clone(),
                });
                break AgentLoopResult::SwitchAgent(provider_id);
            }
            Some(ChatRequest::Shutdown) | None => {
                break AgentLoopResult::Shutdown;
            }
        }
    };

    let _ = connection.close().await;
    result
}

async fn wait_for_switch_or_shutdown(req_rx: &mpsc::Receiver<ChatRequest>) -> AgentLoopResult {
    loop {
        match req_rx.try_recv() {
            Ok(ChatRequest::SwitchAgent { provider_id }) => {
                return AgentLoopResult::SwitchAgent(provider_id);
            }
            Ok(ChatRequest::Shutdown) => return AgentLoopResult::Shutdown,
            Err(mpsc::TryRecvError::Disconnected) => return AgentLoopResult::Shutdown,
            _ => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }
}

fn extract_auth_methods(init_resp: &acp::InitializeResponse) -> Vec<AgentAuthMethod> {
    init_resp
        .auth_methods
        .iter()
        .map(|m| {
            let (terminal_command, terminal_args) = m
                .meta()
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
                id: m.id().0.to_string(),
                name: m.name().to_string(),
                description: m.description().map(|s| s.to_string()),
                terminal_command,
                terminal_args,
            }
        })
        .collect()
}

async fn create_session(
    connection: &acpx::Connection,
    mcp_binary: &Option<PathBuf>,
    pdfium_path: &Option<PathBuf>,
) -> Result<acp::SessionId, String> {
    let mut session_req =
        acp::NewSessionRequest::new(std::env::current_dir().unwrap_or_default());

    if let Some(mcp_bin) = mcp_binary {
        let mut mcp_server = acp::McpServerStdio::new("rotero", mcp_bin.clone());
        if let Some(pdfium) = pdfium_path {
            mcp_server.env.push(acp::EnvVariable::new(
                "PDFIUM_DYNAMIC_LIB_PATH",
                pdfium.to_string_lossy(),
            ));
        }
        session_req
            .mcp_servers
            .push(acp::McpServer::Stdio(mcp_server));
    }

    match connection.new_session(session_req).await {
        Ok(s) => Ok(s.session_id),
        Err(e) => Err(format!("Failed to create session: {e}")),
    }
}

fn handle_session_update(
    evt_tx: &tokio::sync::mpsc::UnboundedSender<ChatEvent>,
    update: &acp::SessionUpdate,
) {
    match update {
        acp::SessionUpdate::AgentMessageChunk(chunk) => {
            if let acp::ContentBlock::Text(text) = &chunk.content {
                let _ = evt_tx.send(ChatEvent::TextDelta(text.text.clone()));
            }
        }
        acp::SessionUpdate::ToolCall(tc) => {
            let _ = evt_tx.send(ChatEvent::ToolCallStarted {
                id: tc.tool_call_id.to_string(),
                title: tc.title.clone(),
            });
        }
        acp::SessionUpdate::ToolCallUpdate(tcu) => {
            let status = match &tcu.fields.status {
                Some(acp::ToolCallStatus::Pending) => ToolStatus::Pending,
                Some(acp::ToolCallStatus::InProgress) => ToolStatus::InProgress,
                Some(acp::ToolCallStatus::Completed) => ToolStatus::Completed,
                Some(acp::ToolCallStatus::Failed) => ToolStatus::Failed,
                Some(_) => return,
                None => return,
            };
            let _ = evt_tx.send(ChatEvent::ToolCallUpdated {
                id: tcu.tool_call_id.to_string(),
                status,
            });
        }
        acp::SessionUpdate::AvailableCommandsUpdate(cmds) => {
            let commands = cmds
                .available_commands
                .iter()
                .map(|c| SlashCommand {
                    name: c.name.clone(),
                    description: c.description.clone(),
                    hint: c.input.as_ref().and_then(|i| match i {
                        acp::AvailableCommandInput::Unstructured(u) => Some(u.hint.clone()),
                        _ => None,
                    }),
                })
                .collect();
            let _ = evt_tx.send(ChatEvent::CommandsAvailable(commands));
        }
        _ => {}
    }
}

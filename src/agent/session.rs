use std::collections::HashMap;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::RequestId;
use agent_client_protocol::schema::v1::{
    AuthenticateRequest, CancelNotification, ContentBlock, Implementation, InitializeRequest,
    ListSessionsRequest, LoadSessionRequest, NewSessionRequest, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionId, SessionNotification, SetSessionConfigOptionRequest,
    TextContent,
};
use agent_client_protocol::{
    AcpAgent, Agent, ByteStreams, Client, ConnectTo, ConnectionTo, Responder,
};

use super::LoopResult;
use super::helpers::{
    agent_working_dir, auth_methods_from_acp, build_mcp_servers, is_auth_error,
    models_from_config_options, session_update_to_events, strip_protocol_tags,
    wait_for_switch_or_shutdown,
};
use super::launch::resolve_launch;
use super::registry::{find_agent, load_registry};
use super::types::{ChatEvent, ChatRequest, PastSession};

pub(crate) fn connect_and_run(
    provider_id: &str,
    req_rx: &mpsc::Receiver<ChatRequest>,
    evt_tx: &tokio::sync::mpsc::UnboundedSender<ChatEvent>,
) -> LoopResult {
    let registry = match load_registry() {
        Ok(r) => r,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "Failed to load agent registry: {e}"
            )));
            return wait_for_switch_or_shutdown(req_rx);
        }
    };
    let Some(agent) = find_agent(&registry, provider_id) else {
        let _ = evt_tx.send(ChatEvent::Error("No ACP agents available".into()));
        return wait_for_switch_or_shutdown(req_rx);
    };
    let provider_id = agent.id.clone();
    let provider_name = agent.name.clone();

    tracing::info!("ACP: connecting to {} ({})", provider_name, provider_id);

    let mut spec = match resolve_launch(agent) {
        Ok(spec) => spec,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "Failed to start {provider_name}: {e}"
            )));
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    let config = crate::sync::engine::SyncConfig::load();
    for (key, val) in config.agent.agent_api_keys {
        if !val.is_empty() {
            spec.env.push((key, val));
        }
    }

    let acp_agent = AcpAgent::new(spec.into_agent_config());
    let outcome = Arc::new(Mutex::new(None::<LoopResult>));
    let pending_permissions: Arc<Mutex<HashMap<String, Responder<RequestPermissionResponse>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let summary = Arc::new(Mutex::new(SummaryBuf::default()));

    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!(
                "Failed to start agent runtime: {e}"
            )));
            return wait_for_switch_or_shutdown(req_rx);
        }
    };

    let ctx = SessionCtx {
        provider_id,
        provider_name,
        evt_tx: evt_tx.clone(),
        pending_permissions,
        summary,
        outcome: outcome.clone(),
    };
    let run = rt.block_on(run_client(acp_agent, req_rx, ctx));

    if let Err(e) = run {
        let _ = evt_tx.send(ChatEvent::Error(e));
        if let Some(result) = outcome.lock().ok().and_then(|mut g| g.take()) {
            return result;
        }
        return wait_for_switch_or_shutdown(req_rx);
    }

    outcome
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .unwrap_or(LoopResult::Shutdown)
}

struct SessionCtx {
    provider_id: String,
    provider_name: String,
    evt_tx: tokio::sync::mpsc::UnboundedSender<ChatEvent>,
    pending_permissions: Arc<Mutex<HashMap<String, Responder<RequestPermissionResponse>>>>,
    summary: Arc<Mutex<SummaryBuf>>,
    outcome: Arc<Mutex<Option<LoopResult>>>,
}

async fn run_client(
    acp_agent: AcpAgent,
    req_rx: &mpsc::Receiver<ChatRequest>,
    ctx: SessionCtx,
) -> Result<(), String> {
    let evt_notify = ctx.evt_tx.clone();
    let evt_perm = ctx.evt_tx.clone();
    let pending_notify = ctx.pending_permissions.clone();
    let summary_notify = ctx.summary.clone();

    Client
        .builder()
        .name("rotero")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                if let Ok(mut sink) = summary_notify.lock()
                    && sink.collecting
                {
                    sink.push_update(&notification.update);
                    return Ok(());
                }
                for event in session_update_to_events(&notification.update) {
                    let _ = evt_notify.send(event);
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                let key = request_id_key(responder.id());
                let request_id =
                    serde_json::to_value(responder.id()).unwrap_or(serde_json::json!(key.clone()));
                if let Ok(mut pending) = pending_notify.lock() {
                    pending.insert(key, responder);
                }
                let tool_title = request
                    .tool_call
                    .fields
                    .title
                    .clone()
                    .unwrap_or_else(|| "Tool call".into());
                let options = request
                    .options
                    .iter()
                    .map(|o| (o.option_id.0.to_string(), o.name.clone()))
                    .collect();
                let _ = evt_perm.send(ChatEvent::PermissionRequest {
                    request_id,
                    tool_title,
                    options,
                });
                Ok(())
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(
            SignalTracked(acp_agent),
            |cx: ConnectionTo<Agent>| async move { drive_session(cx, req_rx, ctx).await },
        )
        .await
        .map_err(|e| e.to_string())
}

/// Spawn the agent ourselves so we know the PID for Ctrl+C, then speak ACP over
/// its stdio. Drop still SIGKILLs the process group on session switch.
struct SignalTracked(AcpAgent);

impl ConnectTo<Client> for SignalTracked {
    async fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> Result<(), agent_client_protocol::Error> {
        let (stdin, stdout, stderr, mut child) = self.0.spawn_process()?;
        let pid = child.id() as i32;
        super::reaper::register(pid);
        tokio::spawn(async move {
            use futures_util::AsyncReadExt as _;
            let mut stderr = stderr;
            let mut buf = [0u8; 8192];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });
        let _guard = ChildReaper {
            pid,
            kill: Some(move || {
                let _ = child.kill();
            }),
        };
        ConnectTo::<Client>::connect_to(ByteStreams::new(stdin, stdout), client).await
    }
}

struct ChildReaper<F: FnOnce()> {
    pid: i32,
    kill: Option<F>,
}

impl<F: FnOnce()> Drop for ChildReaper<F> {
    fn drop(&mut self) {
        super::reaper::unregister(self.pid);
        super::reaper::kill_group(self.pid);
        if let Some(kill) = self.kill.take() {
            kill();
        }
    }
}

async fn drive_session(
    cx: ConnectionTo<Agent>,
    req_rx: &mpsc::Receiver<ChatRequest>,
    ctx: SessionCtx,
) -> Result<(), agent_client_protocol::Error> {
    let SessionCtx {
        provider_id,
        provider_name,
        evt_tx,
        pending_permissions,
        summary,
        outcome,
    } = ctx;
    let evt_tx = &evt_tx;
    let init = cx
        .send_request(InitializeRequest::new(ProtocolVersion::V1).client_info(
            Implementation::new("rotero", env!("CARGO_PKG_VERSION")).title("Rotero Paper Reader"),
        ))
        .block_task()
        .await?;

    tracing::info!("ACP: initialized {provider_name}");
    let auth_methods = auth_methods_from_acp(&init.auth_methods);
    let supports_list = init.agent_capabilities.session_capabilities.list.is_some();
    let load_session = init.agent_capabilities.load_session;
    let _ = evt_tx.send(ChatEvent::Connected {
        auth_methods,
        provider_id: provider_id.clone(),
        provider_name: provider_name.clone(),
        supports_list_sessions: supports_list,
    });

    let cwd = agent_working_dir();
    let mcp_servers = build_mcp_servers();
    let mut session_id = SessionId::new("");
    let mut model_config_id: Option<String> = None;
    let mut deferred_summary: Option<String> = None;

    match cx
        .send_request(NewSessionRequest::new(&cwd).mcp_servers(mcp_servers.clone()))
        .block_task()
        .await
    {
        Ok(created) => {
            session_id = created.session_id;
            tracing::info!("ACP: session created: {session_id}");
            let _ = evt_tx.send(ChatEvent::SessionCreated {
                session_id: session_id.to_string(),
            });
            if let Some(options) = &created.config_options
                && let Some((models, current, config_id)) = models_from_config_options(options)
            {
                model_config_id = Some(config_id.clone());
                let _ = evt_tx.send(ChatEvent::ModelsAvailable {
                    models,
                    current,
                    config_id: Some(config_id),
                });
            }
        }
        Err(e) if is_auth_error(&e.to_string()) => {
            let _ = evt_tx.send(ChatEvent::AuthRequired {
                provider_name: provider_name.clone(),
            });
        }
        Err(e) => {
            let _ = evt_tx.send(ChatEvent::Error(format!("Failed to create session: {e}")));
            return Err(e);
        }
    }

    loop {
        match req_rx.try_recv() {
            Ok(ChatRequest::SendMessage {
                prompt,
                paper_context,
            }) => {
                let full_prompt = match paper_context {
                    Some(ctx) => format!("{ctx}\n\n{prompt}"),
                    None => prompt,
                };
                let sent = cx.send_request(PromptRequest::new(
                    session_id.clone(),
                    vec![ContentBlock::Text(TextContent::new(full_prompt))],
                ));
                let mut fut = std::pin::pin!(sent.block_task());
                let result = loop {
                    tokio::select! {
                        result = &mut fut => break result,
                        _ = tokio::time::sleep(Duration::from_millis(10)) => {
                            match req_rx.try_recv() {
                                Ok(ChatRequest::Cancel) => {
                                    let _ = cx.send_notification(CancelNotification::new(
                                        session_id.clone(),
                                    ));
                                }
                                Ok(ChatRequest::PermissionResponse { request_id, option_id }) => {
                                    resolve_permission(&pending_permissions, request_id, option_id);
                                }
                                Ok(ChatRequest::SummarizeSession { session_id: sid }) => {
                                    deferred_summary = Some(sid);
                                }
                                Ok(ChatRequest::SwitchAgent { provider_id }) => {
                                    let _ = cx.send_notification(CancelNotification::new(
                                        session_id.clone(),
                                    ));
                                    *outcome.lock().unwrap() = Some(LoopResult::SwitchAgent(provider_id));
                                    return Ok(());
                                }
                                Ok(ChatRequest::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                                    *outcome.lock().unwrap() = Some(LoopResult::Shutdown);
                                    return Ok(());
                                }
                                _ => {}
                            }
                        }
                    }
                };
                match result {
                    Ok(_) => {
                        let _ = evt_tx.send(ChatEvent::TurnCompleted);
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("Prompt error: {e}")));
                    }
                }
                if let Some(sid) = deferred_summary.take()
                    && sid == session_id.to_string()
                    && let Some(summary) = request_session_summary(&cx, &session_id, &summary).await
                {
                    let _ = evt_tx.send(ChatEvent::SessionSummary {
                        session_id: sid,
                        summary,
                    });
                }
            }
            Ok(ChatRequest::Cancel) => {
                let _ = cx.send_notification(CancelNotification::new(session_id.clone()));
            }
            Ok(ChatRequest::PermissionResponse {
                request_id,
                option_id,
            }) => {
                resolve_permission(&pending_permissions, request_id, option_id);
            }
            Ok(ChatRequest::ListSessions) => {
                match cx
                    .send_request(ListSessionsRequest::new().cwd(cwd.clone()))
                    .block_task()
                    .await
                {
                    Ok(result) => {
                        let sessions = result
                            .sessions
                            .into_iter()
                            .map(|s| PastSession {
                                session_id: s.session_id.to_string(),
                                cwd: s.cwd.to_string_lossy().into_owned(),
                                title: s.title,
                                updated_at: s.updated_at,
                            })
                            .collect();
                        let _ = evt_tx.send(ChatEvent::SessionList(sessions));
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("List sessions failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::LoadSession {
                session_id: load_id,
                cwd: load_cwd,
            }) => {
                if !load_session {
                    let _ = evt_tx.send(ChatEvent::SessionLoadFailed {
                        session_id: load_id,
                    });
                    continue;
                }
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider_id.clone(),
                });
                let load_cwd = if load_cwd.is_empty() {
                    cwd.clone()
                } else {
                    std::path::PathBuf::from(load_cwd)
                };
                match cx
                    .send_request(
                        LoadSessionRequest::new(SessionId::new(load_id.clone()), load_cwd)
                            .mcp_servers(mcp_servers.clone()),
                    )
                    .block_task()
                    .await
                {
                    Ok(_) => {
                        session_id = SessionId::new(load_id.clone());
                        let _ = evt_tx.send(ChatEvent::SessionCreated {
                            session_id: load_id,
                        });
                    }
                    Err(e) => {
                        tracing::info!("ACP: session {load_id} could not be loaded: {e}");
                        let _ = evt_tx.send(ChatEvent::SessionLoadFailed {
                            session_id: load_id,
                        });
                    }
                }
            }
            Ok(ChatRequest::SummarizeSession { session_id: sid }) => {
                if sid == session_id.to_string()
                    && let Some(summary) = request_session_summary(&cx, &session_id, &summary).await
                {
                    let _ = evt_tx.send(ChatEvent::SessionSummary {
                        session_id: sid,
                        summary,
                    });
                }
            }
            Ok(ChatRequest::SwitchAgent { provider_id }) => {
                let _ = evt_tx.send(ChatEvent::Switching {
                    provider_id: provider_id.clone(),
                });
                *outcome.lock().unwrap() = Some(LoopResult::SwitchAgent(provider_id));
                return Ok(());
            }
            Ok(ChatRequest::SetModel { model_id }) => {
                if let Some(config_id) = &model_config_id {
                    match cx
                        .send_request(SetSessionConfigOptionRequest::new(
                            session_id.clone(),
                            agent_client_protocol::schema::v1::SessionConfigId::new(
                                config_id.clone(),
                            ),
                            model_id.as_str(),
                        ))
                        .block_task()
                        .await
                    {
                        Ok(resp) => {
                            if let Some((models, current, id)) =
                                models_from_config_options(&resp.config_options)
                            {
                                model_config_id = Some(id.clone());
                                let _ = evt_tx.send(ChatEvent::ModelsAvailable {
                                    models,
                                    current,
                                    config_id: Some(id),
                                });
                            }
                        }
                        Err(e) => {
                            let _ = evt_tx.send(ChatEvent::Error(format!("Set model failed: {e}")));
                        }
                    }
                }
            }
            Ok(ChatRequest::Authenticate { method_id }) => {
                let mut meta = serde_json::Map::new();
                meta.insert("headless".into(), serde_json::Value::Bool(true));
                match cx
                    .send_request(AuthenticateRequest::new(method_id).meta(meta))
                    .block_task()
                    .await
                {
                    Ok(_) => {
                        tracing::info!("ACP: auth completed, creating session...");
                        match cx
                            .send_request(
                                NewSessionRequest::new(&cwd).mcp_servers(mcp_servers.clone()),
                            )
                            .block_task()
                            .await
                        {
                            Ok(created) => {
                                session_id = created.session_id;
                                if !session_id.to_string().is_empty() {
                                    let _ = evt_tx.send(ChatEvent::SessionCreated {
                                        session_id: session_id.to_string(),
                                    });
                                }
                            }
                            Err(e) if is_auth_error(&e.to_string()) => {
                                let _ = evt_tx.send(ChatEvent::AuthRequired {
                                    provider_name: provider_name.clone(),
                                });
                            }
                            Err(e) => {
                                let _ =
                                    evt_tx.send(ChatEvent::Error(format!("Session failed: {e}")));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = evt_tx.send(ChatEvent::Error(format!("Auth failed: {e}")));
                    }
                }
            }
            Ok(ChatRequest::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => {
                *outcome.lock().unwrap() = Some(LoopResult::Shutdown);
                return Ok(());
            }
            Err(mpsc::TryRecvError::Empty) => {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }
}

fn request_id_key(id: &RequestId) -> String {
    match id {
        RequestId::Number(n) => n.to_string(),
        RequestId::Str(s) => s.clone(),
        RequestId::Null => "null".into(),
    }
}

fn permission_key(request_id: &serde_json::Value) -> String {
    match request_id {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn resolve_permission(
    pending: &Arc<Mutex<HashMap<String, Responder<RequestPermissionResponse>>>>,
    request_id: serde_json::Value,
    option_id: String,
) {
    let key = permission_key(&request_id);
    let Some(responder) = pending.lock().ok().and_then(|mut g| g.remove(&key)) else {
        return;
    };
    let _ = responder.respond(RequestPermissionResponse::new(
        RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(option_id)),
    ));
}

#[derive(Default)]
struct SummaryBuf {
    collecting: bool,
    text: String,
}

impl SummaryBuf {
    fn start(&mut self) {
        self.collecting = true;
        self.text.clear();
    }

    fn take(&mut self) -> String {
        self.collecting = false;
        std::mem::take(&mut self.text)
    }

    fn push_update(&mut self, update: &agent_client_protocol::schema::v1::SessionUpdate) {
        if let agent_client_protocol::schema::v1::SessionUpdate::AgentMessageChunk(chunk) = update
            && let agent_client_protocol::schema::v1::ContentBlock::Text(t) = &chunk.content
        {
            self.text.push_str(&t.text);
        }
    }
}

async fn request_session_summary(
    cx: &ConnectionTo<Agent>,
    session_id: &SessionId,
    summary: &Arc<Mutex<SummaryBuf>>,
) -> Option<String> {
    const PROMPT: &str = "<rotero-context>\nIn one sentence of at most 120 characters, describe \
what this conversation has been about. Name the specific paper or topic. Reply with the sentence \
only — no preamble, no markdown, no quotes.\n</rotero-context>";
    const TIMEOUT: Duration = Duration::from_secs(45);

    if let Ok(mut sink) = summary.lock() {
        sink.start();
    }
    let sent = cx.send_request(PromptRequest::new(
        session_id.clone(),
        vec![ContentBlock::Text(TextContent::new(PROMPT))],
    ));
    let _ = tokio::time::timeout(TIMEOUT, sent.block_task()).await;
    let buf = summary.lock().ok()?.take();
    let text: String = strip_protocol_tags(&buf)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .trim_matches('"')
        .chars()
        .take(200)
        .collect();
    (!text.is_empty()).then_some(text)
}

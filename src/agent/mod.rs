mod connection;
mod helpers;
mod install;
pub(crate) mod node;
pub(crate) mod reaper;
mod session;
pub mod types;

use std::sync::mpsc;

use session::connect_and_run;
use types::{AGENT_PROVIDERS, ChatEvent, ChatRequest};

pub(crate) enum LoopResult {
    SwitchAgent(String),
    Shutdown,
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
        .find(|p| p.id == config.agent.agent_provider)
        .unwrap_or(&AGENT_PROVIDERS[0]);

    // Connecting eagerly meant every fresh install downloaded ~50MB of Node.js
    // and ran `npm install` on first launch, whether or not the user ever opened
    // the chat panel — and any failure in that path became a startup failure.
    //
    // Wait for something that actually needs the agent. The request is forwarded
    // into a channel the session loop drains first, so the message that woke us
    // is answered rather than dropped.
    let (feed_tx, feed_rx) = mpsc::channel();
    match req_rx.recv() {
        Ok(ChatRequest::Shutdown) | Err(_) => return,
        Ok(first) => {
            if feed_tx.send(first).is_err() {
                return;
            }
        }
    }

    // Everything arriving after the first request is relayed on a helper thread,
    // so the session loop sees one uninterrupted stream.
    std::thread::spawn(move || {
        while let Ok(req) = req_rx.recv() {
            if feed_tx.send(req).is_err() {
                break;
            }
        }
    });
    let req_rx = feed_rx;

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

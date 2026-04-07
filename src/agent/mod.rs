mod connection;
mod helpers;
mod install;
pub(crate) mod node;
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

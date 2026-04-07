mod message;
mod panel;
mod resize_handle;
mod toggle;

use dioxus::prelude::*;

use crate::agent::types::{ChatRequest, ChatState, AgentStatus, ChatMessage, ChatRole, MessageContent};
use crate::state::app_state::{LibraryState, PdfTabManager};

pub use panel::ChatPanel;
pub use toggle::ChatToggleButton;
pub use resize_handle::ResizeHandle;

/// Channel wrapper for sending requests to the ACP agent thread.
#[derive(Clone, Copy)]
pub struct AgentChannel {
    pub inner: Signal<Option<std::sync::mpsc::Sender<ChatRequest>>>,
}

impl AgentChannel {
    pub fn send(&self, req: ChatRequest) {
        if let Some(tx) = self.inner.read().as_ref() {
            tracing::info!("AgentChannel: sending request");
            let _ = tx.send(req);
        } else {
            tracing::warn!("AgentChannel: no sender available");
        }
    }
}

fn get_active_paper_id(lib_state: &LibraryState, tab_mgr: &PdfTabManager) -> Option<String> {
    tab_mgr
        .active_tab_id
        .and_then(|tid| tab_mgr.tabs.iter().find(|t| t.id == tid))
        .and_then(|t| t.paper_id.clone())
        .or_else(|| lib_state.selected_paper_id.clone())
}

fn build_paper_context(lib_state: &LibraryState, tab_mgr: &PdfTabManager) -> Option<String> {
    let paper_id = get_active_paper_id(lib_state, tab_mgr)?;
    let paper = lib_state.papers.iter().find(|p| p.id.as_deref() == Some(paper_id.as_str()))?;

    Some(format!(
        "<rotero-context>\nI'm currently looking at this paper in my library:\n\
         Title: {}\nAuthors: {}\nYear: {}\nDOI: {}\nPaper ID: {}\n\
         You can use the rotero MCP tools to search my library, \
         read this paper's annotations, extract PDF text, etc.\n</rotero-context>",
        paper.title,
        paper.authors.join(", "),
        paper.year.map(|y| y.to_string()).unwrap_or_default(),
        paper.doi.as_deref().unwrap_or(""),
        paper_id,
    ))
}

fn get_context_paper_title(
    lib_state: &LibraryState,
    tab_mgr: &PdfTabManager,
) -> Option<String> {
    let paper_id = get_active_paper_id(lib_state, tab_mgr)?;
    lib_state
        .papers
        .iter()
        .find(|p| p.id.as_deref() == Some(paper_id.as_str()))
        .map(|p| p.title.clone())
}

fn do_send(
    chat_state: &mut Signal<ChatState>,
    agent_channel: &AgentChannel,
    lib_state: &Signal<LibraryState>,
    tab_mgr: &Signal<PdfTabManager>,
) {
    let input = chat_state.read().input_text.trim().to_string();
    if input.is_empty() {
        return;
    }

    chat_state.with_mut(|s| {
        s.messages.push(ChatMessage {
            role: ChatRole::User,
            content: vec![MessageContent::Text(input.clone())],
            timestamp: chrono::Utc::now(),
        });
        s.input_text.clear();
        s.status = AgentStatus::Streaming;
        s.show_command_picker = false;
    });

    let paper_context = build_paper_context(&lib_state.read(), &tab_mgr.read());

    agent_channel.send(ChatRequest::SendMessage {
        prompt: input,
        paper_context,
    });
}

mod message;
mod panel;
mod resize_handle;
mod toggle;

use dioxus::prelude::*;

use crate::agent::types::{
    AgentStatus, ChatMessage, ChatRequest, ChatRole, ChatState, MessageContent,
};
use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use rotero_db::chat_sessions::ChatSubject;

pub use panel::ChatPanel;
pub use resize_handle::ResizeHandle;
pub use toggle::ChatToggleButton;

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
        .or_else(|| lib_state.single_selected_id().cloned())
}

/// Standing guidance sent with every message: steer the agent to Rotero's own
/// search/import tools so its results match the UI search bar and land cleanly
/// in the library, rather than falling back to generic web search.
const SEARCH_GUIDANCE: &str = "\
When finding papers, prefer the rotero MCP tools over generic web search: \
`search_online` (searches OpenAlex, arXiv, and Semantic Scholar together and \
returns papers in the library's format), `find_pdf` (locates an open-access PDF \
URL), `download_pdf`, and `add_paper` to import a result into my library. \
Use `search_papers` to search papers already in my library.";

fn build_paper_context(lib_state: &LibraryState, tab_mgr: &PdfTabManager) -> Option<String> {
    let paper_block = get_active_paper_id(lib_state, tab_mgr)
        .and_then(|paper_id| {
            lib_state
                .papers
                .iter()
                .find(|p| p.id.as_deref() == Some(paper_id.as_str()))
                .map(|paper| (paper_id, paper))
        })
        .map(|(paper_id, paper)| {
            format!(
                "\nI'm currently looking at this paper in my library:\n\
                 Title: {}\nAuthors: {}\nYear: {}\nDOI: {}\nPaper ID: {}\n\
                 You can use the rotero MCP tools to read this paper's annotations, \
                 extract PDF text, etc.",
                paper.title,
                paper.author_names().join(", "),
                paper.year.map(|y| y.to_string()).unwrap_or_default(),
                paper.doi.as_deref().unwrap_or(""),
                paper_id,
            )
        })
        .unwrap_or_default();

    Some(format!(
        "<rotero-context>\n{SEARCH_GUIDANCE}{paper_block}\n</rotero-context>"
    ))
}

/// What a conversation started right now would be about.
///
/// An open PDF wins: it is the paper being read, whatever the library list
/// happens to have selected. Several papers selected is one subject — a group —
/// rather than several conversations, and a collection being browsed with
/// nothing selected is the collection itself.
pub(crate) fn current_subject(
    lib_state: &LibraryState,
    tab_mgr: &PdfTabManager,
) -> Option<ChatSubject> {
    if let Some(paper_id) = tab_mgr
        .active_tab_id
        .and_then(|tid| tab_mgr.tabs.iter().find(|t| t.id == tid))
        .and_then(|t| t.paper_id.clone())
    {
        return Some(ChatSubject::Paper(paper_id));
    }
    if lib_state.selection_count() > 1 {
        return Some(ChatSubject::Group(
            lib_state.selected_paper_ids.iter().cloned().collect(),
        ));
    }
    if let Some(paper_id) = lib_state.single_selected_id() {
        return Some(ChatSubject::Paper(paper_id.clone()));
    }
    if let LibraryView::Collection(id) = &lib_state.view {
        return Some(ChatSubject::Collection(id.clone()));
    }
    None
}

fn get_context_paper_title(lib_state: &LibraryState, tab_mgr: &PdfTabManager) -> Option<String> {
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

    // The subject is known now but the session id is not, so hold it until
    // `SessionCreated` arrives with something to key it to.
    let subject = current_subject(&lib_state.read(), &tab_mgr.read());

    chat_state.with_mut(|s| {
        s.messages.push(ChatMessage::new(
            ChatRole::User,
            vec![MessageContent::Text(input.clone())],
        ));
        s.input_text.clear();
        s.status = AgentStatus::Streaming;
        s.show_command_picker = false;
        if s.pending_subject.is_none() {
            s.pending_subject = subject;
        }
    });

    let paper_context = build_paper_context(&lib_state.read(), &tab_mgr.read());

    agent_channel.send(ChatRequest::SendMessage {
        prompt: input,
        paper_context,
    });
}

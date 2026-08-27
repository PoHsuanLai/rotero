mod message;
mod panel;
mod resize_handle;
mod resume;
mod toggle;

use dioxus::prelude::*;

use crate::agent::types::{
    AgentStatus, ChatMessage, ChatRequest, ChatRole, ChatState, MessageContent,
};
use crate::state::app_state::{LibraryState, LibraryView, PdfTabManager};
use rotero_db::Database;
use rotero_db::chat_sessions::ChatSubject;

pub use panel::ChatPanel;
pub use resize_handle::ResizeHandle;
pub use resume::{SubjectFollower, subject_label, subject_of_row, switch_to};
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

/// How many papers a group conversation names in its context block.
///
/// A large selection would otherwise crowd out the conversation itself; the
/// agent can look the rest up by id through the MCP tools.
const GROUP_CONTEXT_LIMIT: usize = 25;

/// The papers of a multi-paper subject, as a compact list for the prompt.
fn group_context(lib_state: &LibraryState, paper_ids: &[String]) -> String {
    let listed: Vec<String> = paper_ids
        .iter()
        .filter_map(|id| {
            lib_state
                .papers
                .iter()
                .find(|p| p.id.as_deref() == Some(id.as_str()))
        })
        .take(GROUP_CONTEXT_LIMIT)
        .map(|p| {
            format!(
                "- {} ({}) — {} [Paper ID: {}]",
                p.title,
                p.year
                    .map(|y| y.to_string())
                    .unwrap_or_else(|| "n.d.".into()),
                p.formatted_authors(),
                p.id.as_deref().unwrap_or(""),
            )
        })
        .collect();
    if listed.is_empty() {
        return String::new();
    }
    let omitted = paper_ids.len().saturating_sub(listed.len());
    let note = if omitted > 0 {
        format!("\n(and {omitted} more — use the rotero MCP tools to list them)")
    } else {
        String::new()
    };
    format!(
        "\nI'm asking about these papers together:\n{}{note}",
        listed.join("\n")
    )
}

fn build_paper_context(
    lib_state: &LibraryState,
    tab_mgr: &PdfTabManager,
    subject: Option<&ChatSubject>,
) -> Option<String> {
    // A conversation about several papers has to name them all, or the agent
    // only ever sees whichever one happens to be open.
    if let Some(ChatSubject::Group(ids)) = subject {
        let block = group_context(lib_state, ids);
        return Some(format!(
            "<rotero-context>\n{SEARCH_GUIDANCE}{block}\n</rotero-context>"
        ));
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::app_state::PdfTab;
    use rotero_models::Paper;

    fn paper(id: &str, title: &str) -> Paper {
        Paper {
            id: Some(id.to_string()),
            title: title.to_string(),
            ..Default::default()
        }
    }

    fn library() -> LibraryState {
        LibraryState {
            papers: vec![paper("p1", "First"), paper("p2", "Second")],
            ..Default::default()
        }
    }

    /// An open PDF is what the user is reading, whatever the list has selected.
    #[test]
    fn an_open_pdf_wins_over_the_selection() {
        let mut lib = library();
        lib.select_one("p2".into());
        let mut tabs = PdfTabManager::default();
        let id = tabs.next_id();
        let mut tab = PdfTab::new(id, "/tmp/a.pdf".into(), "First".into(), 1.0, 4, 1.0);
        tab.paper_id = Some("p1".into());
        tabs.open_tab(tab);

        assert_eq!(
            current_subject(&lib, &tabs),
            Some(ChatSubject::Paper("p1".into()))
        );
    }

    /// Several papers selected is one conversation about the group, not one
    /// conversation per paper.
    #[test]
    fn a_multi_selection_is_a_single_group_subject() {
        let mut lib = library();
        lib.select_one("p1".into());
        lib.toggle_select("p2".into());

        let subject = current_subject(&lib, &PdfTabManager::default()).unwrap();
        assert!(matches!(subject, ChatSubject::Group(ref ids) if ids.len() == 2));
    }

    #[test]
    fn a_browsed_collection_is_the_subject_when_nothing_is_selected() {
        let mut lib = library();
        lib.view = LibraryView::Collection("c1".into());

        assert_eq!(
            current_subject(&lib, &PdfTabManager::default()),
            Some(ChatSubject::Collection("c1".into()))
        );
    }

    /// Nothing open and nothing selected is a general chat, not a subject.
    #[test]
    fn browsing_the_whole_library_has_no_subject() {
        assert_eq!(current_subject(&library(), &PdfTabManager::default()), None);
    }

    /// The agent only ever sees the paper that happens to be open unless the
    /// group names them all.
    #[test]
    fn a_group_context_names_every_paper() {
        let block = group_context(&library(), &["p1".to_string(), "p2".to_string()]);
        assert!(block.contains("Paper ID: p1"));
        assert!(block.contains("Paper ID: p2"));
        assert!(block.contains("First"));
        assert!(block.contains("Second"));
    }

    /// A large selection must not crowd out the conversation itself.
    #[test]
    fn a_large_group_is_capped_and_says_so() {
        let mut lib = LibraryState::default();
        let ids: Vec<String> = (0..GROUP_CONTEXT_LIMIT + 5)
            .map(|i| {
                let id = format!("p{i}");
                lib.papers.push(paper(&id, &format!("Paper {i}")));
                id
            })
            .collect();

        let block = group_context(&lib, &ids);
        assert_eq!(block.matches("Paper ID:").count(), GROUP_CONTEXT_LIMIT);
        assert!(block.contains("and 5 more"));
    }

    /// A group conversation stays about its group even while one of its papers
    /// is open, or the agent would silently narrow to that paper.
    #[test]
    fn the_subject_beats_the_open_pdf_when_building_context() {
        let lib = library();
        let mut tabs = PdfTabManager::default();
        let id = tabs.next_id();
        let mut tab = PdfTab::new(id, "/tmp/a.pdf".into(), "First".into(), 1.0, 4, 1.0);
        tab.paper_id = Some("p1".into());
        tabs.open_tab(tab);

        let group = ChatSubject::Group(vec!["p1".into(), "p2".into()]);
        let context = build_paper_context(&lib, &tabs, Some(&group)).unwrap();
        assert!(context.contains("Paper ID: p2"));
        assert!(context.contains("asking about these papers together"));
    }
}

fn do_send(
    chat_state: &mut Signal<ChatState>,
    agent_channel: &AgentChannel,
    lib_state: &Signal<LibraryState>,
    tab_mgr: &Signal<PdfTabManager>,
    db: &Database,
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

    // A cheap label, so a conversation is never nameless: the agent summary is
    // better but costs a round trip, and may not arrive at all.
    //
    // A new conversation has no session id yet — the agent reports one only
    // after this message is sent — so hold the label until it does rather than
    // writing it against nothing.
    let first_message = chat_state
        .read()
        .messages
        .iter()
        .filter(|m| m.role == ChatRole::User)
        .count()
        == 1;
    if first_message {
        let fallback: String = input.chars().take(120).collect();
        let existing = chat_state.read().current_session_id.clone();
        match existing {
            Some(session_id) => {
                let db = db.clone();
                spawn(async move {
                    let _ = db.set_chat_session_summary(&session_id, &fallback).await;
                });
            }
            None => chat_state.with_mut(|s| s.pending_summary = Some(fallback)),
        }
    }

    // The conversation's own subject wins over what is on screen: a group chat
    // stays about its group even while one of its papers is open.
    let context_subject = chat_state
        .read()
        .current_subject
        .clone()
        .or_else(|| chat_state.read().pending_subject.clone());
    let paper_context =
        build_paper_context(&lib_state.read(), &tab_mgr.read(), context_subject.as_ref());

    agent_channel.send(ChatRequest::SendMessage {
        prompt: input,
        paper_context,
    });

    // Queued behind the message above: the agent thread handles requests in
    // order, so asking first would summarize a conversation that hasn't
    // happened yet.
    //
    // Only for a conversation that already has a session id. A brand new one
    // gets its label from `pending_summary` instead, since there is nothing to
    // address the request to yet.
    if first_message && let Some(session_id) = chat_state.read().current_session_id.clone() {
        agent_channel.send(ChatRequest::SummarizeSession { session_id });
    }
}

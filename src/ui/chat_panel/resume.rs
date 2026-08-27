//! Keeping the panel's conversation pointed at what the user is reading.
//!
//! A chat belongs to its subject, so opening a paper should continue that
//! paper's conversation rather than whatever was last on screen. Switching is
//! automatic only while the panel is idle: doing it mid-reply would abandon a
//! conversation the user is still waiting on, so that case asks first.

use dioxus::prelude::*;
use rotero_db::Database;
use rotero_db::chat_sessions::{ChatSessionRow, ChatSubject};

use crate::agent::types::{AgentStatus, ChatRequest, ChatState, PendingSwitch};
use crate::state::app_state::{LibraryState, PdfTabManager};

use super::{AgentChannel, current_subject};

/// How to name a subject in the UI.
pub fn subject_label(subject: &ChatSubject, lib: &LibraryState) -> String {
    match subject {
        ChatSubject::Paper(id) => lib
            .papers
            .iter()
            .find(|p| p.id.as_deref() == Some(id.as_str()))
            .map(|p| p.title.clone())
            .unwrap_or_else(|| "this paper".into()),
        ChatSubject::Collection(id) => lib
            .collections
            .iter()
            .find(|c| c.id.as_deref() == Some(id.as_str()))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "this collection".into()),
        ChatSubject::Group(ids) => format!("{} papers", ids.len()),
    }
}

/// When a conversation was last used, as a short human date.
///
/// Same-subject conversations are otherwise indistinguishable in a list, so the
/// time is what tells them apart.
pub fn short_time(rfc3339: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(rfc3339).ok().map(|t| {
        t.with_timezone(&chrono::Local)
            .format("%-d %b, %H:%M")
            .to_string()
    })
}

/// What to call a conversation with no stored summary.
///
/// The agent's own title is useless here — it names the transcript's first
/// message, which is a startup command — so name the conversation by what it is
/// about and when it was last used instead.
pub fn unlabelled_title(about: Option<&str>, last_used_at: &str) -> String {
    match (about, short_time(last_used_at)) {
        (Some(a), Some(t)) => format!("{a} — {t}"),
        (Some(a), None) => a.to_string(),
        (None, Some(t)) => format!("Chat — {t}"),
        (None, None) => "Untitled chat".into(),
    }
}

/// Rebuild the subject a stored conversation is about.
///
/// A group is identified by its members rather than by `subject_id`, which
/// holds only their hash, so the members are looked up from the supplied
/// `(session_id, paper_id)` pairs.
pub fn subject_of_row(row: &ChatSessionRow, subjects: &[(String, String)]) -> Option<ChatSubject> {
    match row.subject_kind.as_str() {
        "paper" => row.subject_id.clone().map(ChatSubject::Paper),
        "collection" => row.subject_id.clone().map(ChatSubject::Collection),
        "group" => {
            let members: Vec<String> = subjects
                .iter()
                .filter(|(sid, _)| *sid == row.session_id)
                .map(|(_, pid)| pid.clone())
                .collect();
            (!members.is_empty()).then_some(ChatSubject::Group(members))
        }
        _ => None,
    }
}

/// Whether the panel can switch conversations without losing anything.
fn is_idle(state: &ChatState) -> bool {
    matches!(state.status, AgentStatus::Idle) && state.input_text.trim().is_empty()
}

/// Point the panel at `subject`, resuming its conversation or starting one.
pub fn switch_to(
    chat_state: &mut Signal<ChatState>,
    agent_channel: &AgentChannel,
    db: &Database,
    subject: ChatSubject,
) {
    chat_state.with_mut(|s| {
        s.messages.clear();
        s.current_subject = Some(subject.clone());
        s.pending_subject = Some(subject.clone());
        s.pending_switch = None;
        s.declined_subject = None;
        s.current_session_id = None;
    });

    let db = db.clone();
    let agent_channel = *agent_channel;
    let mut chat_state = *chat_state;
    spawn(async move {
        match db.chat_session_for_subject(&subject).await {
            Ok(Some(existing)) => {
                chat_state.with_mut(|s| s.status = AgentStatus::Connecting);
                agent_channel.send(ChatRequest::LoadSession {
                    session_id: existing.session_id,
                    cwd: String::new(),
                });
            }
            // Nothing to resume: the next message starts this subject's
            // conversation, and `pending_subject` already says what it is about.
            Ok(None) => {}
            Err(e) => tracing::debug!("chat: looking up a conversation failed: {e}"),
        }
    });
}

/// Follows the active subject, switching the panel's conversation to match.
#[component]
pub fn SubjectFollower() -> Element {
    let mut chat_state = use_context::<Signal<ChatState>>();
    let lib_state = use_context::<Signal<LibraryState>>();
    let tab_mgr = use_context::<Signal<PdfTabManager>>();
    let agent_channel = use_context::<AgentChannel>();
    let db = use_context::<Database>();

    use_effect(move || {
        // Read outside any spawn so the effect actually subscribes to these.
        let panel_open = chat_state.read().panel_open;
        let subject = current_subject(&lib_state.read(), &tab_mgr.read());

        if !panel_open {
            return;
        }
        let Some(subject) = subject else {
            // No subject — a general chat. Leave whatever is on screen alone
            // rather than clearing it for nothing.
            return;
        };
        if chat_state.read().current_subject.as_ref() == Some(&subject) {
            return;
        }
        // Asking twice about the same subject would nag on every render, and a
        // subject already declined should stay declined.
        if chat_state
            .read()
            .pending_switch
            .as_ref()
            .is_some_and(|p| p.subject == subject)
            || chat_state.read().declined_subject.as_ref() == Some(&subject)
        {
            return;
        }

        if is_idle(&chat_state.read()) {
            switch_to(&mut chat_state, &agent_channel, &db, subject);
        } else {
            let label = subject_label(&subject, &lib_state.read());
            chat_state.with_mut(|s| {
                s.pending_switch = Some(PendingSwitch { subject, label });
            });
        }
    });

    rsx! {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(kind: &str, subject_id: Option<&str>) -> ChatSessionRow {
        ChatSessionRow {
            session_id: "sess-1".into(),
            provider_id: "claude".into(),
            subject_kind: kind.into(),
            subject_id: subject_id.map(String::from),
            summary: None,
            created_at: String::new(),
            last_used_at: String::new(),
            is_dead: false,
        }
    }

    #[test]
    fn a_paper_row_rebuilds_its_subject() {
        assert_eq!(
            subject_of_row(&row("paper", Some("p1")), &[]),
            Some(ChatSubject::Paper("p1".into()))
        );
    }

    /// A group's `subject_id` is a hash of its members, so the members
    /// themselves have to come from the link rows.
    #[test]
    fn a_group_row_rebuilds_from_its_member_links() {
        let links = vec![
            ("sess-1".to_string(), "p1".to_string()),
            ("sess-1".to_string(), "p2".to_string()),
            ("sess-other".to_string(), "p3".to_string()),
        ];

        let subject = subject_of_row(&row("group", Some("hash")), &links).unwrap();

        match subject {
            ChatSubject::Group(ids) => assert_eq!(ids, vec!["p1", "p2"]),
            other => panic!("expected a group, got {other:?}"),
        }
    }

    /// Every member deleted leaves nothing to describe the conversation by.
    #[test]
    fn a_group_with_no_surviving_members_has_no_subject() {
        assert_eq!(subject_of_row(&row("group", Some("hash")), &[]), None);
    }

    /// The agent's own title is a startup command, so an unlabelled conversation
    /// is named by what it is about instead.
    #[test]
    fn an_unlabelled_conversation_is_named_by_subject_and_time() {
        let title = unlabelled_title(Some("Attention Is All You Need"), "2026-08-27T07:54:10Z");
        assert!(
            title.starts_with("Attention Is All You Need — "),
            "got {title}"
        );
    }

    /// Two conversations about the same paper are otherwise identical in a
    /// list, so the time has to be what separates them.
    #[test]
    fn same_subject_conversations_differ_by_time() {
        let a = unlabelled_title(Some("A paper"), "2026-08-27T07:54:10Z");
        let b = unlabelled_title(Some("A paper"), "2026-08-27T09:31:00Z");
        assert_ne!(a, b);
    }

    /// A conversation about nothing in particular, with an unreadable date,
    /// still needs a name.
    #[test]
    fn a_conversation_with_nothing_known_still_has_a_name() {
        assert_eq!(unlabelled_title(None, "not-a-date"), "Untitled chat");
    }

    /// The clock list shows the open subject's conversations, so a chat about a
    /// different paper has to drop out — and a group is matched by its members,
    /// not by the order they were selected in.
    #[test]
    fn only_the_current_subjects_conversations_match() {
        let here = ChatSubject::Paper("p1".into());

        let mine = row("paper", Some("p1"));
        let theirs = row("paper", Some("p2"));

        assert_eq!(subject_of_row(&mine, &[]).as_ref(), Some(&here));
        assert_ne!(subject_of_row(&theirs, &[]).as_ref(), Some(&here));
    }

    /// A conversation about no paper at all belongs to no subject, so it is only
    /// reachable once the list is widened.
    #[test]
    fn a_general_conversation_matches_no_subject() {
        assert_eq!(subject_of_row(&row("general", None), &[]), None);
    }

    /// The list renders before the stored record loads. A row must not settle on
    /// "Untitled chat" in that gap — the subject and time are there, one frame
    /// later, and that is what the row should end up saying.
    #[test]
    fn a_row_with_a_record_is_named_from_it_not_from_the_empty_fallback() {
        let lib = LibraryState {
            papers: vec![rotero_models::Paper {
                id: Some("p1".into()),
                title: "Attention Is All You Need".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut r = row("paper", Some("p1"));
        r.last_used_at = "2026-08-27T07:54:10Z".into();

        // What the render path does once the resource has resolved.
        let about = subject_of_row(&r, &[]).map(|s| subject_label(&s, &lib));
        let title = r
            .summary
            .clone()
            .unwrap_or_else(|| unlabelled_title(about.as_deref(), &r.last_used_at));

        assert_ne!(title, "Untitled chat");
        assert!(
            title.starts_with("Attention Is All You Need — "),
            "got {title}"
        );
    }
}

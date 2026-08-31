//! The subject→conversation mapping, and its deliberate absence from sync.

mod common;

use rotero_db::Database;
use rotero_db::chat_sessions::{ChatSessionRow, ChatSubject};
use rotero_models::Paper;

async fn insert(db: &Database, title: &str) -> String {
    let paper = Paper {
        title: title.to_string(),
        ..Default::default()
    };
    db.insert_paper(&paper).await.unwrap()
}

fn row(session_id: &str, subject: &ChatSubject) -> ChatSessionRow {
    ChatSessionRow {
        session_id: session_id.to_string(),
        provider_id: "claude".into(),
        subject_kind: subject.kind().to_string(),
        subject_id: Some(subject.id()),
        summary: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        last_used_at: "2026-01-01T00:00:00Z".into(),
        is_dead: false,
    }
}

#[tokio::test]
async fn a_paper_subject_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "Attention Is All You Need").await;
    let subject = ChatSubject::Paper(paper.clone());

    db.upsert_chat_session(&row("sess-1", &subject), &subject.paper_ids())
        .await
        .unwrap();

    let found = db.chat_session_for_subject(&subject).await.unwrap();
    assert_eq!(found.map(|r| r.session_id), Some("sess-1".to_string()));
    assert_eq!(
        db.chat_session_paper_ids("sess-1").await.unwrap(),
        vec![paper]
    );
}

#[tokio::test]
async fn a_group_is_identified_by_its_members_not_their_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let a = insert(&db, "A").await;
    let b = insert(&db, "B").await;
    let c = insert(&db, "C").await;

    let selected = ChatSubject::Group(vec![a.clone(), b.clone(), c.clone()]);
    db.upsert_chat_session(&row("sess-group", &selected), &selected.paper_ids())
        .await
        .unwrap();

    // Re-selecting the same papers in a different order is the same subject.
    let reselected = ChatSubject::Group(vec![c.clone(), a.clone(), b.clone()]);
    assert_eq!(selected.id(), reselected.id());
    let found = db.chat_session_for_subject(&reselected).await.unwrap();
    assert_eq!(found.map(|r| r.session_id), Some("sess-group".to_string()));

    // A different set is a different subject, and has no conversation yet.
    let narrower = ChatSubject::Group(vec![a, b]);
    assert!(
        db.chat_session_for_subject(&narrower)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn upsert_keeps_the_original_age_and_never_erases_a_summary() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "A").await;
    let subject = ChatSubject::Paper(paper);

    db.upsert_chat_session(&row("sess-1", &subject), &subject.paper_ids())
        .await
        .unwrap();
    db.set_chat_session_summary("sess-1", "Explains the attention mechanism")
        .await
        .unwrap();

    // A later upsert that doesn't know the summary must not clear it.
    let mut later = row("sess-1", &subject);
    later.last_used_at = "2026-06-01T00:00:00Z".into();
    db.upsert_chat_session(&later, &subject.paper_ids())
        .await
        .unwrap();

    let found = db
        .chat_session_for_subject(&subject)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        found.summary.as_deref(),
        Some("Explains the attention mechanism")
    );
    assert_eq!(found.created_at, "2026-01-01T00:00:00Z");
    assert_eq!(found.last_used_at, "2026-06-01T00:00:00Z");
}

#[tokio::test]
async fn linking_is_idempotent_and_ignores_unknown_papers() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "A").await;
    let subject = ChatSubject::Paper(paper.clone());
    db.upsert_chat_session(&row("sess-1", &subject), &[])
        .await
        .unwrap();

    db.link_chat_session_paper("sess-1", &paper, false)
        .await
        .unwrap();
    db.link_chat_session_paper("sess-1", &paper, false)
        .await
        .unwrap();
    // An id the agent invented is not a library paper, so it is dropped.
    db.link_chat_session_paper("sess-1", "not-a-real-paper", false)
        .await
        .unwrap();

    assert_eq!(
        db.chat_session_paper_ids("sess-1").await.unwrap(),
        vec![paper]
    );
}

/// The bug this guards: an agent answering a question searches the library, and
/// every paper it reads used to be linked as though the conversation were about
/// it — so one chat about one paper appeared on the detail panel of every paper
/// the search happened to return.
#[tokio::test]
async fn a_paper_the_agent_merely_read_does_not_claim_the_conversation() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let discussed = insert(&db, "The paper being read").await;
    let stumbled_on = insert(&db, "A search result").await;

    let subject = ChatSubject::Paper(discussed.clone());
    db.upsert_chat_session(&row("sess-1", &subject), &subject.paper_ids())
        .await
        .unwrap();
    db.link_chat_session_paper("sess-1", &stumbled_on, false)
        .await
        .unwrap();

    assert_eq!(
        db.chat_sessions_for_paper(&discussed).await.unwrap().len(),
        1
    );
    assert!(
        db.chat_sessions_for_paper(&stumbled_on)
            .await
            .unwrap()
            .is_empty()
    );
    // Both are still on record, so the conversation can be traced.
    assert_eq!(db.chat_session_paper_ids("sess-1").await.unwrap().len(), 2);
}

/// A paper that is the subject stays the subject: the agent re-reading it
/// mid-conversation must not demote it to an incidental mention.
#[tokio::test]
async fn an_incidental_mention_cannot_demote_a_subject() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "A").await;
    let subject = ChatSubject::Paper(paper.clone());
    db.upsert_chat_session(&row("sess-1", &subject), &subject.paper_ids())
        .await
        .unwrap();

    db.link_chat_session_paper("sess-1", &paper, false)
        .await
        .unwrap();

    assert_eq!(db.chat_sessions_for_paper(&paper).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_dead_session_stops_being_offered_but_stays_on_record() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "A").await;
    let subject = ChatSubject::Paper(paper.clone());
    db.upsert_chat_session(&row("sess-1", &subject), &subject.paper_ids())
        .await
        .unwrap();

    db.mark_chat_session_dead("sess-1").await.unwrap();

    assert!(
        db.chat_session_for_subject(&subject)
            .await
            .unwrap()
            .is_none()
    );
    assert!(db.chat_sessions_for_paper(&paper).await.unwrap().is_empty());
    // The link survives, so the conversation is still inspectable.
    assert_eq!(
        db.chat_session_paper_ids("sess-1").await.unwrap(),
        vec![paper]
    );
}

#[tokio::test]
async fn a_deleted_paper_drops_out_of_its_conversations() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let kept = insert(&db, "Kept").await;
    let removed = insert(&db, "Removed").await;
    let subject = ChatSubject::Group(vec![kept.clone(), removed.clone()]);
    db.upsert_chat_session(&row("sess-group", &subject), &subject.paper_ids())
        .await
        .unwrap();

    // Papers are tombstoned rather than removed, so the cascade never fires —
    // the reads have to exclude dead papers themselves.
    db.delete_paper(&removed).await.unwrap();

    assert_eq!(
        db.chat_session_paper_ids("sess-group").await.unwrap(),
        vec![kept.clone()]
    );
    assert!(
        db.chat_sessions_for_paper(&removed)
            .await
            .unwrap()
            .is_empty()
    );
    // The conversation is still reachable from the paper that remains.
    assert_eq!(db.chat_sessions_for_paper(&kept).await.unwrap().len(), 1);
}

#[tokio::test]
async fn chat_tables_are_not_synced() {
    // The whole point of keeping these local: a session id minted by the agent
    // on this machine means nothing on another device, so a synced row would
    // name a conversation that cannot be resumed.
    for table in rotero_db::sync_schema::SYNCED_TABLES {
        assert_ne!(table.name, "chat_sessions");
        assert_ne!(table.name, "chat_session_papers");
        assert_ne!(table.name, "chat_messages");
    }
}

/// Transcripts survive and can be retrieved across sessions and providers.
#[tokio::test]
async fn transcript_persists_across_sessions_and_providers() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "Deep Residual Learning").await;
    let subject = ChatSubject::Paper(paper);

    // Initial session with Claude
    let claude_row = row("sess-claude", &subject);
    db.upsert_chat_session(&claude_row, &subject.paper_ids())
        .await
        .unwrap();

    let msg1 = rotero_db::chat_sessions::ChatMessageRecord {
        id: "sess-claude:1".into(),
        session_id: "sess-claude".into(),
        seq: 1,
        role: "user".into(),
        content_json: r#"[{"Text":"Explain residual connections"}]"#.into(),
        created_at: "2026-08-31T12:00:00Z".into(),
    };
    let msg2 = rotero_db::chat_sessions::ChatMessageRecord {
        id: "sess-claude:2".into(),
        session_id: "sess-claude".into(),
        seq: 2,
        role: "assistant".into(),
        content_json: r#"[{"Text":"Residual connections allow gradients to flow directly..."}]"#
            .into(),
        created_at: "2026-08-31T12:00:10Z".into(),
    };

    db.append_chat_message(&msg1).await.unwrap();
    db.append_chat_message(&msg2).await.unwrap();

    // Query conversation for subject
    let session = db
        .chat_session_for_subject(&subject)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(session.session_id, "sess-claude");

    let messages = db
        .chat_messages_for_session(&session.session_id)
        .await
        .unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");

    // Switch provider to Grok: update the session with new provider and append message
    let mut grok_row = session.clone();
    grok_row.provider_id = "grok".into();
    grok_row.last_used_at = "2026-08-31T12:05:00Z".into();
    db.upsert_chat_session(&grok_row, &subject.paper_ids())
        .await
        .unwrap();

    let msg3 = rotero_db::chat_sessions::ChatMessageRecord {
        id: "sess-claude:3".into(),
        session_id: "sess-claude".into(),
        seq: 3,
        role: "user".into(),
        content_json: r#"[{"Text":"How does it compare to Highway Networks?"}]"#.into(),
        created_at: "2026-08-31T12:05:05Z".into(),
    };
    db.append_chat_message(&msg3).await.unwrap();

    let updated_messages = db.chat_messages_for_session("sess-claude").await.unwrap();
    assert_eq!(updated_messages.len(), 3);
    assert_eq!(updated_messages[2].seq, 3);
}

/// The bug this guards: the row is created when the agent announces the session
/// and the label is written when the user sends a message, and both writes are
/// spawned independently. An UPDATE that lost the race updated nothing, so every
/// stored summary stayed null and the chat list fell back to the agent's own
/// uninformative title.
#[tokio::test]
async fn a_summary_written_before_its_row_exists_is_not_lost() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert(&db, "A").await;
    let subject = ChatSubject::Paper(paper);

    // The label lands first, with no row to update.
    db.set_chat_session_summary("sess-1", "What does this paper claim?")
        .await
        .unwrap();
    // The session announcement follows, and must not erase it.
    db.upsert_chat_session(&row("sess-1", &subject), &subject.paper_ids())
        .await
        .unwrap();

    let found = db
        .chat_session_for_subject(&subject)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        found.summary.as_deref(),
        Some("What does this paper claim?")
    );
    // The placeholder columns must not survive the real upsert.
    assert_eq!(found.subject_kind, "paper");
    assert_eq!(found.provider_id, "claude");
}

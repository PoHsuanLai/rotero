//! The v17 migration clears out conversations that never happened.
//!
//! An earlier build filed a row when the agent opened a session, which it does
//! on connect — so every launch left one behind whether or not anything was
//! said. The cleanup has to remove those without touching a real conversation.

mod common;

use rotero_db::chat_sessions::{ChatSessionRow, ChatSubject};
use rotero_models::Paper;

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

/// Rewind so reopening runs the v17 block.
async fn rewind_to_v16(dir: &std::path::Path) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(16)],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn empty_sessions_are_cleared_and_real_ones_kept() {
    let dir = tempfile::tempdir().unwrap();
    let paper = {
        let db = common::open_test_db(dir.path()).await;
        let paper = db
            .insert_paper(&Paper {
                title: "A".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        let subject = ChatSubject::Paper(paper.clone());

        // A real conversation: has a subject paper linked.
        db.upsert_chat_session(&row("real", &subject), &subject.paper_ids())
            .await
            .unwrap();
        // A conversation with a label but no papers is still real.
        db.upsert_chat_session(&row("labelled", &subject), &[])
            .await
            .unwrap();
        db.set_chat_session_summary("labelled", "Explain this")
            .await
            .unwrap();
        // The shells: no label, no papers.
        db.upsert_chat_session(&row("empty-1", &subject), &[])
            .await
            .unwrap();
        db.upsert_chat_session(&row("empty-2", &subject), &[])
            .await
            .unwrap();
        paper
    };
    rewind_to_v16(dir.path()).await;

    let db = common::open_test_db(dir.path()).await;

    let ids: Vec<String> = db
        .all_chat_sessions()
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.session_id)
        .collect();
    assert!(ids.contains(&"real".to_string()), "got {ids:?}");
    assert!(ids.contains(&"labelled".to_string()), "got {ids:?}");
    assert!(!ids.contains(&"empty-1".to_string()), "got {ids:?}");
    assert!(!ids.contains(&"empty-2".to_string()), "got {ids:?}");

    // The surviving conversation is still reachable from its paper.
    assert_eq!(db.chat_sessions_for_paper(&paper).await.unwrap().len(), 1);
}

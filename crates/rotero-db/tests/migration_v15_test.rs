//! The v15 migration must add the chat tables to a library that predates them.
//!
//! `CREATE_TABLES` covers a fresh database, so a bug in the migration block is
//! invisible to every other test — they all start from a new library. This one
//! rewinds a real library to v14 and reopens it, which is the upgrade path an
//! existing user actually takes.

mod common;

use rotero_db::chat_sessions::{ChatSessionRow, ChatSubject};
use rotero_models::Paper;

/// Drop the chat tables and rewind the version, as a genuinely pre-v15 library is.
async fn rewind_to_v14(dir: &std::path::Path) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    conn.execute("DROP TABLE IF EXISTS chat_session_papers", ())
        .await
        .unwrap();
    conn.execute("DROP TABLE IF EXISTS chat_sessions", ())
        .await
        .unwrap();
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(14)],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn reopening_a_v14_library_creates_the_chat_tables() {
    let dir = tempfile::tempdir().unwrap();
    let paper = {
        let db = common::open_test_db(dir.path()).await;
        db.insert_paper(&Paper {
            title: "A".into(),
            ..Default::default()
        })
        .await
        .unwrap()
    };
    rewind_to_v14(dir.path()).await;

    let db = common::open_test_db(dir.path()).await;

    // The tables exist and are usable, not merely present.
    let subject = ChatSubject::Paper(paper.clone());
    db.upsert_chat_session(
        &ChatSessionRow {
            session_id: "sess-1".into(),
            provider_id: "claude".into(),
            subject_kind: subject.kind().to_string(),
            subject_id: Some(subject.id()),
            summary: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            last_used_at: "2026-01-01T00:00:00Z".into(),
            is_dead: false,
        },
        &subject.paper_ids(),
    )
    .await
    .unwrap();

    assert_eq!(
        db.chat_session_for_subject(&subject)
            .await
            .unwrap()
            .map(|r| r.session_id),
        Some("sess-1".to_string())
    );
    assert_eq!(db.chat_sessions_for_paper(&paper).await.unwrap().len(), 1);
}

#[tokio::test]
async fn the_upgrade_leaves_the_library_healthy() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = common::open_test_db(dir.path()).await;
        db.insert_paper(&Paper {
            title: "A".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    }
    rewind_to_v14(dir.path()).await;

    let db = common::open_test_db(dir.path()).await;

    // The new tables are local-only, so they must not register as a sync
    // problem — health derives its expectations from SYNCED_TABLES.
    let problems = rotero_db::health::verify_database_health(&db).await;
    assert!(problems.is_empty(), "unexpected problems: {problems:?}");
}

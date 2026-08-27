//! The v16 migration must add `is_subject` to a library created at v15.
//!
//! The column is declared in `CREATE_TABLES` and in the v15 block's
//! `CREATE TABLE IF NOT EXISTS`, so a fresh library and a pre-v15 upgrade both
//! get it — and every other test starts from one of those. A library created
//! *at* v15 already has the table, so both statements are no-ops for it and
//! only the ALTER can add the column. That is the path a real user was on, and
//! the version stamped to 16 without the column ever arriving.

mod common;

use rotero_db::chat_sessions::{ChatSessionRow, ChatSubject};
use rotero_models::Paper;

/// Rebuild `chat_session_papers` without `is_subject` and rewind to 15, which is
/// exactly the shape a library created by the v15 build has.
async fn rewind_to_v15(dir: &std::path::Path) {
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
    conn.execute(
        "CREATE TABLE chat_session_papers (
            session_id TEXT NOT NULL REFERENCES chat_sessions(session_id) ON DELETE CASCADE,
            paper_id   TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
            PRIMARY KEY (session_id, paper_id)
        )",
        (),
    )
    .await
    .unwrap();
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(15)],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn reopening_a_v15_library_adds_the_is_subject_column() {
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
    rewind_to_v15(dir.path()).await;

    // Reopening runs the migrations, as launching the app does.
    let db = common::open_test_db(dir.path()).await;

    // The column is what every subject query filters on, so a conversation is
    // only findable by its paper if the ALTER landed.
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

    assert_eq!(db.chat_sessions_for_paper(&paper).await.unwrap().len(), 1);
}

/// A library an earlier build stamped 16 without adding the column reports
/// itself up to date, so a version-guarded migration would skip it forever.
/// Reopening has to repair it regardless of what the version says.
#[tokio::test]
async fn a_library_stamped_16_without_the_column_still_gets_it() {
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

    // The exact broken state: table without the column, version already at 16.
    rewind_to_v15(dir.path()).await;
    {
        let db_path = dir.path().join("rotero.db");
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

    let db = common::open_test_db(dir.path()).await;

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

    assert_eq!(db.chat_sessions_for_paper(&paper).await.unwrap().len(), 1);
}

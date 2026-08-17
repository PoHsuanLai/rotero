//! Shared fixtures for the database integration tests.
//!
//! `open_test_db` was copy-pasted into eight test files. That is how the bug
//! these tests missed stayed hidden: every copy went through `Database::open`,
//! so the suite only ever exercised the one construction path that was correct.
//! Keeping the fixture in one place makes it obvious which path is under test.

#![allow(dead_code)]

use rotero_db::Database;

/// Open a fully initialized library in `dir`.
///
/// This is what the app does, so tests share its behaviour: schema migrations,
/// CRR initialization, and the untracked-row backfill all run.
pub async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf())
        .await
        .expect("opening a test library must succeed")
}

/// The `col_ver` of a tracked column, or 0 when it has no clock entry.
///
/// Re-adopting a row that is already tracked adds no change rows and alters no
/// local data — it only rewinds this number. That makes `col_ver` the only
/// observable that can catch the regression, which is why tests assert on it
/// rather than on `changes_since().len()`.
pub async fn col_ver(db: &Database, table: &str, pk: &str, column: &str) -> i64 {
    let sql = format!("SELECT col_ver FROM {table}__crr_clock WHERE pk = ?1 AND col_name = ?2");
    let mut rows = db
        .conn()
        .query(
            &sql,
            [
                turso::Value::Text(pk.to_string()),
                turso::Value::Text(column.to_string()),
            ],
        )
        .await
        .expect("reading a clock table must succeed");
    match rows.next().await {
        Ok(Some(row)) => row
            .get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0),
        _ => 0,
    }
}

/// Attach a tag to a paper the way a build with broken tracking did: the row
/// commits, the clock never hears about it.
pub async fn insert_untracked_paper_tag(dir: &std::path::Path, paper_id: &str, tag_id: &str) {
    let conn = raw_connect(dir).await;
    conn.execute(
        "INSERT INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)",
        [
            turso::Value::Text(paper_id.to_string()),
            turso::Value::Text(tag_id.to_string()),
        ],
    )
    .await
    .expect("inserting an untracked junction row must succeed");
}

/// Forget that the repair ever ran, so reopening re-scans.
///
/// Models the flag-key bump that makes libraries stamped by the buggy version
/// repair themselves once.
pub async fn clear_backfill_flags(dir: &std::path::Path) {
    let conn = raw_connect(dir).await;
    conn.execute("DELETE FROM app_flags WHERE key LIKE 'crr_backfill_%'", ())
        .await
        .expect("clearing the backfill flag must succeed");
}

/// A raw connection to a library, bypassing `Database::open` so no repair runs.
async fn raw_connect(dir: &std::path::Path) -> turso::Connection {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .expect("building a turso database must succeed");
    raw.connect().expect("connecting must succeed")
}

/// A library with the app tables but no CRR metadata — the shape a build that
/// skipped `crr.init()` left behind. Writes to it commit and then fail change
/// tracking.
pub async fn open_uninitialized_db(dir: &std::path::Path) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .expect("building a turso database must succeed");
    let conn = raw.connect().expect("connecting must succeed");
    rotero_db::schema::initialize_db(&conn)
        .await
        .expect("creating the app tables must succeed");
}

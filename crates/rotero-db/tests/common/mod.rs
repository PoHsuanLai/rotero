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

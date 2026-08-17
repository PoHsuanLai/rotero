//! Guards on the schema version, both directions.
//!
//! Sync means a library can arrive from a machine running a different build.
//! Opening a newer one with older code would run its rows through migrations
//! written against columns that may since have been renamed or dropped — and the
//! result syncs back.

use rotero_db::Database;

/// Set the recorded schema version, bypassing the migration path.
async fn force_schema_version(dir: &std::path::Path, version: i64) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(version)],
    )
    .await
    .unwrap();
    drop(conn);
    drop(raw);
}

/// A library from a newer build must be refused, with a message that says what
/// to do about it.
#[tokio::test]
async fn a_newer_library_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    Database::open(dir.path().to_path_buf()).await.unwrap();
    force_schema_version(dir.path(), 9_999).await;

    let err = match Database::open(dir.path().to_path_buf()).await {
        Ok(_) => panic!("opening a newer library must fail"),
        Err(e) => e,
    };

    assert!(
        err.contains("newer version") && err.contains("9999"),
        "the error must name the problem and the version, got: {err}"
    );
}

/// The guard must not fire on the version this build writes, or every launch
/// after an upgrade would refuse to open.
#[tokio::test]
async fn the_current_version_opens_normally() {
    let dir = tempfile::tempdir().unwrap();
    Database::open(dir.path().to_path_buf()).await.unwrap();

    // Reopening exercises the same guard against a populated library.
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    assert!(
        rotero_db::health::verify_database_health(&db)
            .await
            .is_empty()
    );
}

/// An older library still migrates forward. The guard is one-directional; making
/// it symmetric would break every upgrade.
#[tokio::test]
async fn an_older_library_still_migrates() {
    let dir = tempfile::tempdir().unwrap();
    Database::open(dir.path().to_path_buf()).await.unwrap();
    force_schema_version(dir.path(), 1).await;

    let db = Database::open(dir.path().to_path_buf())
        .await
        .expect("an older library must migrate rather than be refused");
    assert!(
        rotero_db::health::verify_database_health(&db)
            .await
            .is_empty(),
        "a migrated library must be healthy"
    );
}

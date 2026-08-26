//! Pins the invariant that every constructible `Database` is sync-capable.
//!
//! The regression these guard against: a second database-construction path ran
//! `initialize_db` without `crr.init()`, so every write committed its row and
//! then failed change tracking. Tags and notes vanished on reload and nothing
//! synced. The existing suite passed throughout, because every fixture was built
//! through `Database::open`, which was correct.

use rotero_db::Database;
use rotero_db::health::{HealthIssue, verify_database_health};

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

#[tokio::test]
async fn open_produces_a_healthy_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    assert_eq!(
        verify_database_health(&db).await,
        vec![],
        "Database::open must satisfy every structural invariant"
    );
}

/// The detector must actually detect. A database with the app tables but no sync
/// metadata is exactly what shipped, and it has to come back unhealthy — the
/// other two callers of `verify_database_health` (the startup preflight and the
/// bundle smoke test) are only worth trusting if this fails when it should.
#[tokio::test]
async fn a_schema_only_database_is_repaired_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rotero.db");

    // Build the broken shape directly: app tables, no sync initialization.
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    rotero_db::schema::initialize_db(&conn).await.unwrap();
    drop(conn);
    drop(raw);

    // Reopening through the supported constructor must repair it.
    let db = open_test_db(dir.path()).await;
    assert_eq!(
        verify_database_health(&db).await,
        vec![],
        "reopening a schema-only database through Database::open must repair it"
    );
}

/// Health checking must not be hardcoded to a subset of tables: a synced table
/// that has lost the columns a merge compares has to be reported. Guards against
/// the check silently narrowing as the schema grows.
#[tokio::test]
async fn a_table_without_sync_columns_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    // A table predating the sync columns looks exactly like this.
    db.conn()
        .execute("DROP TABLE saved_searches", ())
        .await
        .unwrap();
    db.conn()
        .execute(
            "CREATE TABLE saved_searches (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                query TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();

    let issues = verify_database_health(&db).await;
    assert!(
        issues.iter().any(|i| matches!(
            i,
            HealthIssue::MissingSyncColumns { table, .. } if table == "saved_searches"
        )),
        "expected MissingSyncColumns for `saved_searches`, got {issues:?}"
    );
}

/// `attach_readonly` must not repair what it inspects.
///
/// A checker built on `Database::open` reports every library healthy, because
/// opening initializes the missing metadata before the check runs — the smoke
/// test would then be permanently green and worthless. Pinned because the
/// difference between the two constructors is invisible at the call site.
#[tokio::test]
async fn attach_readonly_does_not_repair_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rotero.db");

    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    rotero_db::schema::initialize_db(&conn).await.unwrap();
    drop(conn);
    drop(raw);

    let attached = Database::attach_readonly(dir.path().to_path_buf())
        .await
        .unwrap();
    assert!(
        !verify_database_health(&attached).await.is_empty(),
        "attach_readonly must report the database as-is, not initialize it"
    );
    drop(attached);

    // And the supported constructor still repairs it, so the two differ.
    let opened = open_test_db(dir.path()).await;
    assert_eq!(verify_database_health(&opened).await, vec![]);
}

/// Tags survive a reopen.
///
/// This is the user-visible symptom of the shipped bug: `add_tag_to_paper`
/// committed its row and then failed change tracking, the UI discarded the
/// `Err`, and the tag was gone on the next launch. Asserted end-to-end because
/// the structural checks above cannot show that the data actually round-trips.
#[tokio::test]
async fn tags_survive_a_reopen() {
    let dir = tempfile::tempdir().unwrap();

    let paper_id = {
        let db = open_test_db(dir.path()).await;
        let paper = rotero_models::Paper {
            title: "Tagged".into(),
            ..Default::default()
        };
        let paper_id = db.insert_paper(&paper).await.expect("insert_paper");
        let tag_id = db
            .get_or_create_tag("important", Some("#ff0000"))
            .await
            .expect("get_or_create_tag");
        db.add_tag_to_paper(&paper_id, &tag_id)
            .await
            .expect("add_tag_to_paper must not fail");
        paper_id
    };

    let db = open_test_db(dir.path()).await;
    let tags = db
        .list_tags_for_paper(&paper_id)
        .await
        .expect("list_tags_for_paper");
    assert_eq!(
        tags.len(),
        1,
        "the tag must still be attached after a reopen"
    );
    assert_eq!(tags[0].name, "important");
}

/// A missing device identity short-circuits: it is the cause, and reporting it
/// alongside nine follow-on per-table issues would bury it.
#[tokio::test]
async fn an_absent_device_identity_reports_only_the_root_cause() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    // Remove the identity the library actually stores. Checking the handle's
    // cached copy instead would report every read-only inspection as broken,
    // which is what `attach_readonly` legitimately produces.
    db.conn()
        .execute("DELETE FROM crr_site_id", ())
        .await
        .unwrap();

    assert_eq!(
        verify_database_health(&db).await,
        vec![HealthIssue::DeviceIdMissing]
    );
}

/// Rows whose clock was never written are the shape a broken build leaves
/// behind: the rows are all there, none of them can win a merge, and every
/// structural check for the table's *existence* passes.
///
/// This was reported healthy under the old engine, which made the check blind to
/// the exact failure it was written for.
#[tokio::test]
async fn rows_without_a_sync_clock_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    db.insert_paper(&rotero_models::Paper {
        title: "Present but untracked".into(),
        ..Default::default()
    })
    .await
    .unwrap();

    assert!(
        verify_database_health(&db).await.is_empty(),
        "a normally-inserted paper must be healthy"
    );

    // Wipe the clock, leaving the row and the table itself in place.
    db.conn()
        .execute("UPDATE papers SET updated_at = 0, updated_by = ''", ())
        .await
        .unwrap();

    let issues = verify_database_health(&db).await;
    assert!(
        issues.contains(&HealthIssue::UnstampedRows {
            table: "papers".to_string(),
            rows: 1,
        }),
        "a row that cannot win a merge must be reported, got {issues:?}"
    );
}

/// A tombstone with no clock is a deletion that will never reach a peer.
#[tokio::test]
async fn tombstones_without_a_clock_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let paper = db
        .insert_paper(&rotero_models::Paper {
            title: "Deleted but stuck".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    db.delete_paper(&paper).await.unwrap();

    assert!(
        verify_database_health(&db).await.is_empty(),
        "a normally-deleted paper must be healthy"
    );

    db.conn()
        .execute("UPDATE papers SET updated_at = 0 WHERE deleted = 1", ())
        .await
        .unwrap();

    let issues = verify_database_health(&db).await;
    assert!(
        issues.iter().any(|i| matches!(
            i,
            HealthIssue::TombstoneWithoutClock { table, .. } if table == "papers"
        )),
        "a deletion that cannot propagate must be reported, got {issues:?}"
    );
}

/// An empty library is not damage.
#[tokio::test]
async fn empty_tables_are_not_mistaken_for_damage() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    assert!(
        verify_database_health(&db).await.is_empty(),
        "a fresh library has no rows to stamp and must still be healthy"
    );
}

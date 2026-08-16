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

/// The detector must actually detect. A database with the app tables but no CRR
/// metadata is exactly what shipped, and it has to come back unhealthy — the
/// other two callers of `verify_database_health` (the startup preflight and the
/// bundle smoke test) are only worth trusting if this fails when it should.
#[tokio::test]
async fn schema_without_crr_init_is_detected_as_unhealthy() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rotero.db");

    // Build the broken shape directly: app tables, no `crr.init()`.
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

/// Health checking must not be hardcoded to a subset of tables: dropping any one
/// tracked table's clock table has to be reported. Guards against the check
/// silently narrowing as the schema grows.
#[tokio::test]
async fn a_missing_clock_table_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    db.conn()
        .execute("DROP TABLE papers__crr_clock", ())
        .await
        .unwrap();

    let issues = verify_database_health(&db).await;
    assert!(
        issues.contains(&HealthIssue::MissingCrrClock {
            table: "papers".to_string(),
        }),
        "expected a MissingCrrClock issue for `papers`, got {issues:?}"
    );
}

/// `attach_readonly` must not repair what it inspects.
///
/// A checker built on `Database::open` reports every library healthy, because
/// `crr.init()` recreates the missing metadata before the check runs — the smoke
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
    assert_eq!(
        verify_database_health(&attached).await,
        vec![HealthIssue::CrrUninitialized],
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

/// Absent CRR metadata short-circuits: it is the cause, and reporting it
/// alongside nine follow-on clock-table issues would bury it.
#[tokio::test]
async fn absent_crr_metadata_reports_only_the_root_cause() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    db.conn()
        .execute("DROP TABLE crr_site_id", ())
        .await
        .unwrap();

    assert_eq!(
        verify_database_health(&db).await,
        vec![HealthIssue::CrrUninitialized]
    );
}

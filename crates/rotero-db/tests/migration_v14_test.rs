//! The v14 migration must leave every existing row able to win a merge.
//!
//! Under last-writer-wins, a row stamped `updated_at = 0` loses every
//! comparison forever — it is present locally and can never propagate. A
//! half-applied backfill is therefore silent, permanent data loss rather than a
//! visible error, so the migration's postconditions are asserted directly rather
//! than inferred from it not returning `Err`.

use rotero_db::Database;
use rotero_db::sync_schema::SYNCED_TABLES;

/// Rewind a library to a pre-LWW schema version so the next open migrates it.
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
}

async fn scalar(db: &Database, sql: &str) -> i64 {
    let mut rows = db.conn().query(sql, ()).await.unwrap();
    rows.next()
        .await
        .unwrap()
        .and_then(|r| r.get_value(0).ok())
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(-1)
}

/// Populate a library with a row in every synced table.
async fn populate(db: &Database) -> String {
    let paper = db
        .insert_paper(&rotero_models::Paper {
            title: "Migrated".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let tag = db.get_or_create_tag("kept", None).await.unwrap();
    let collection = db
        .insert_collection(&rotero_models::Collection::new("Shelf".into()))
        .await
        .unwrap();
    db.add_tag_to_paper(&paper, &tag).await.unwrap();
    db.add_paper_to_collection(&paper, &collection).await.unwrap();
    db.insert_note(&rotero_models::Note::new(paper.clone(), "Note".into()))
        .await
        .unwrap();
    paper
}

/// Every row must come out of the migration stamped and able to win a merge.
#[tokio::test]
async fn migration_stamps_every_existing_row() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    populate(&db).await;
    drop(db);

    force_schema_version(dir.path(), 13).await;
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    for table in SYNCED_TABLES {
        let unstamped = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM {} WHERE updated_at = 0 OR updated_by = ''",
                table.name
            ),
        )
        .await;
        assert_eq!(
            unstamped, 0,
            "`{}` has rows that would lose every merge forever",
            table.name
        );
    }
}

/// Migration seeds must lose to a genuine edit made after the migration.
///
/// Seeding at `now` would mean the second device to migrate outranks the first
/// on every row, so its copy of the whole library would silently win. The seed
/// is backdated a day to keep it below any real edit.
#[tokio::test]
async fn seeded_timestamps_are_backdated() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    populate(&db).await;
    drop(db);

    force_schema_version(dir.path(), 13).await;
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    let now_ms = chrono::Utc::now().timestamp_millis();
    // Tables with no timestamp of their own take the backdated seed.
    for table in ["collections", "tags", "paper_tags", "paper_collections"] {
        let newest = scalar(&db, &format!("SELECT MAX(updated_at) FROM {table}")).await;
        assert!(
            newest < now_ms - 3_600_000,
            "`{table}` seeded at {newest}, within an hour of now ({now_ms}) — \
             a peer migrating later would win the whole table"
        );
    }
}

/// Rows carrying their own edit time keep it, rather than all collapsing to one
/// seed value.
#[tokio::test]
async fn rows_with_timestamps_seed_from_them() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let paper = populate(&db).await;

    // Give this paper a distinctly old edit time.
    db.conn()
        .execute(
            "UPDATE papers SET date_modified = '2020-01-02T03:04:05Z' WHERE id = ?1",
            [turso::Value::Text(paper.clone())],
        )
        .await
        .unwrap();
    drop(db);

    force_schema_version(dir.path(), 13).await;
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    let stamped = scalar(
        &db,
        &format!("SELECT updated_at FROM papers WHERE id = '{paper}'"),
    )
    .await;
    let expected = chrono::DateTime::parse_from_rfc3339("2020-01-02T03:04:05Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        stamped, expected,
        "a paper's own date_modified must seed its sync clock, so an old paper \
         does not outrank one edited yesterday"
    );
}

/// The device keeps its identity across the migration.
#[tokio::test]
async fn migration_preserves_device_identity() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let before = db.crr().site_id().await.unwrap();
    populate(&db).await;
    drop(db);

    force_schema_version(dir.path(), 13).await;
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let after = db.crr().site_id().await.unwrap();

    assert_eq!(
        before, after,
        "the device id must survive the migration; a new one would make every \
         existing row look like it came from a different device"
    );
}

/// Re-running the migration must not change anything.
#[tokio::test]
async fn migration_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    populate(&db).await;
    drop(db);

    force_schema_version(dir.path(), 13).await;
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let first = scalar(&db, "SELECT SUM(updated_at) FROM papers").await;
    drop(db);

    force_schema_version(dir.path(), 13).await;
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let second = scalar(&db, "SELECT SUM(updated_at) FROM papers").await;

    assert_eq!(
        first, second,
        "re-running the migration must not restamp rows that already carry a clock"
    );
}

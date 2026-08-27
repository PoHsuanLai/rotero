//! The v18 migration adds `papers.pdf_sha256`, the content identity PDF sync
//! addresses a shared file by.
//!
//! A library written before it existed has papers with a `pdf_path` and no
//! hash. Those rows have to survive the migration intact and become
//! backfillable — a row that arrived without a hash and could not acquire one
//! would have its PDF silently excluded from sync, which is the failure the
//! column exists to end.

mod common;

use rotero_models::Paper;

/// Rewind so reopening runs the v18 block.
async fn rewind_to_v17(dir: &std::path::Path) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(17)],
    )
    .await
    .unwrap();
}

/// A pre-v18 paper keeps its path, and is offered to the backfill.
#[tokio::test]
async fn a_paper_imported_before_the_column_is_queued_for_hashing() {
    let dir = tempfile::tempdir().unwrap();

    let (with_pdf, without_pdf) = {
        let db = common::open_test_db(dir.path()).await;
        let with_pdf = db
            .insert_paper(&Paper {
                title: "Has a PDF".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        // Written the way a pre-v18 build would: a path, no hash.
        db.update_pdf_path(&with_pdf, "2024/Has a PDF - Author.pdf", None)
            .await
            .unwrap();

        let without_pdf = db
            .insert_paper(&Paper {
                title: "No PDF".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        (with_pdf, without_pdf)
    };

    rewind_to_v17(dir.path()).await;
    let db = common::open_test_db(dir.path()).await;

    // The path survived the migration.
    let pending = db.list_papers_needing_pdf_hashes().await.unwrap();
    assert_eq!(
        pending,
        vec![(with_pdf.clone(), "2024/Has a PDF - Author.pdf".to_string())],
        "the pre-existing paper must be queued for hashing, and only it"
    );

    // A paper with no PDF is not queued — there is nothing to hash.
    assert!(
        !pending.iter().any(|(id, _)| id == &without_pdf),
        "a paper with no PDF must not be queued"
    );

    // And it has no hash yet, so sync correctly skips it until the backfill runs.
    assert_eq!(db.pdf_sha256(&with_pdf).await.unwrap(), None);
}

/// Backfilling a hash must not republish the row.
///
/// The clock decides which copy of a row wins a merge. Hashing a file that has
/// not changed is a local repair of a row that predates the column, not an
/// edit — stamping it would let a backfill on one device outrank a real
/// metadata edit made on another, which is how retiring a tag by stamped rename
/// blanked out real names in the previous pass.
#[tokio::test]
async fn backfilling_a_hash_does_not_stamp_the_sync_clock() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let id = db
        .insert_paper(&Paper {
            title: "Backfilled".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    db.update_pdf_path(&id, "2024/Backfilled - Author.pdf", None)
        .await
        .unwrap();

    let clock_before = clock_of(&db, &id).await;
    db.set_pdf_sha256(&id, &"a".repeat(64)).await.unwrap();
    let clock_after = clock_of(&db, &id).await;

    assert_eq!(
        clock_before, clock_after,
        "recording a hash for an unchanged file must not republish the paper"
    );
    assert_eq!(
        db.pdf_sha256(&id).await.unwrap(),
        Some("a".repeat(64)),
        "but the hash must still be stored"
    );
}

/// Setting a path *with* a hash is a real edit and must stamp.
///
/// The counterpart to the test above: attaching a PDF changes what the row
/// says, and a peer has to hear about it.
#[tokio::test]
async fn attaching_a_pdf_does_stamp_the_sync_clock() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let id = db
        .insert_paper(&Paper {
            title: "Attached".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let clock_before = clock_of(&db, &id).await;
    db.update_pdf_path(&id, "2024/Attached - Author.pdf", Some(&"b".repeat(64)))
        .await
        .unwrap();
    let clock_after = clock_of(&db, &id).await;

    assert!(
        clock_after > clock_before,
        "attaching a PDF must publish the row: {clock_before} -> {clock_after}"
    );
}

async fn clock_of(db: &rotero_db::Database, id: &str) -> i64 {
    let mut rows = db
        .conn()
        .query(
            "SELECT updated_at FROM papers WHERE id = ?1",
            [turso::Value::Text(id.to_string())],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("the paper must exist");
    row.get_value(0)
        .ok()
        .and_then(|v| v.as_integer().copied())
        .unwrap_or_default()
}

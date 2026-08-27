//! Local writes must be stamped so they can win a merge.
//!
//! A row whose clock is never written loses every comparison forever: present
//! locally, unable to propagate. These assert the stamping itself, since the
//! merge that consumes it does not exist until the next step.

mod common;

use rotero_db::Database;
use rotero_db::clock::Pk;

async fn scalar(db: &Database, sql: &str) -> i64 {
    let mut rows = db.conn().query(sql, ()).await.unwrap();
    rows.next()
        .await
        .unwrap()
        .and_then(|r| r.get_value(0).ok())
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(-1)
}

async fn insert_paper(db: &Database, title: &str) -> String {
    db.insert_paper(&rotero_models::Paper {
        title: title.into(),
        ..Default::default()
    })
    .await
    .unwrap()
}

/// Every locally written row carries this device's stamp.
#[tokio::test]
async fn local_writes_are_stamped() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let paper = insert_paper(&db, "Stamped").await;
    let tag = db.get_or_create_tag("t", None).await.unwrap();
    db.add_tag_to_paper(&paper, &tag).await.unwrap();

    for (table, count) in [("papers", 1), ("tags", 1), ("paper_tags", 1)] {
        let stamped = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM {table} WHERE updated_at > 0 AND updated_by = '{}'",
                db.device_id()
            ),
        )
        .await;
        assert_eq!(stamped, count, "`{table}` must carry this device's stamp");
    }
}

/// A second edit must outrank the first, even inside the same millisecond.
///
/// Wall-clock alone is not enough: two writes in the same millisecond would tie,
/// and a backwards clock jump would let a device's own newer edit lose to its
/// own older one.
#[tokio::test]
async fn each_edit_outranks_the_last() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "First").await;

    let mut previous = scalar(
        &db,
        &format!("SELECT updated_at FROM papers WHERE id = '{paper}'"),
    )
    .await;

    for _ in 0..5 {
        db.touch("papers", Pk::Single(&paper)).await.unwrap();
        let now = scalar(
            &db,
            &format!("SELECT updated_at FROM papers WHERE id = '{paper}'"),
        )
        .await;
        assert!(
            now > previous,
            "each edit must strictly outrank the last ({now} !> {previous})"
        );
        previous = now;
    }
}

/// Even with the clock wound back, a device's own edit must still win.
#[tokio::test]
async fn a_backwards_clock_cannot_lose_to_itself() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "Future").await;

    // Stamp the row far in the future, as a skewed peer or a wound-forward
    // clock would.
    let future = chrono::Utc::now().timestamp_millis() + 86_400_000;
    db.conn()
        .execute(
            "UPDATE papers SET updated_at = ?1 WHERE id = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::Integer(future),
                turso::Value::Text(paper.clone()),
            ]),
        )
        .await
        .unwrap();

    db.touch("papers", Pk::Single(&paper)).await.unwrap();

    let after = scalar(
        &db,
        &format!("SELECT updated_at FROM papers WHERE id = '{paper}'"),
    )
    .await;
    assert!(
        after > future,
        "a local edit must outrank the row's own future stamp, or it would \
         silently never propagate again"
    );
}

/// Deleting tombstones the row rather than removing it.
#[tokio::test]
async fn deletes_leave_a_stamped_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "Doomed").await;

    db.delete_paper(&paper).await.unwrap();

    let tombstoned = scalar(
        &db,
        &format!(
            "SELECT COUNT(*) FROM papers WHERE id = '{paper}' AND deleted = 1 AND updated_at > 0"
        ),
    )
    .await;
    assert_eq!(
        tombstoned, 1,
        "the row must survive as a stamped tombstone; a hard delete leaves \
         nothing to publish and the peer would resurrect it"
    );
    assert!(
        db.list_papers().await.unwrap().is_empty(),
        "and must not be visible locally"
    );
}

/// Re-adding a removed tag revives the membership rather than silently failing.
#[tokio::test]
async fn re_adding_a_tag_clears_its_tombstone() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let paper = insert_paper(&db, "Tagged").await;
    let tag = db.get_or_create_tag("on-off", None).await.unwrap();

    db.add_tag_to_paper(&paper, &tag).await.unwrap();
    db.remove_tag_from_paper(&paper, &tag).await.unwrap();
    assert!(db.list_tags_for_paper(&paper).await.unwrap().is_empty());

    db.add_tag_to_paper(&paper, &tag).await.unwrap();
    assert_eq!(
        db.list_tags_for_paper(&paper).await.unwrap().len(),
        1,
        "re-adding must clear the tombstone; `INSERT OR IGNORE` would do \
         nothing and leave the membership deleted while appearing to succeed"
    );
}

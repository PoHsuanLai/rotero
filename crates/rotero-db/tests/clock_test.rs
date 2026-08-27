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

/// A tag can be created again under a name a deleted tag still holds.
///
/// Found by the generated schedules in `sync_props`. `tags.name` is UNIQUE
/// across dead rows too, so a tombstoned tag keeps its name: looking only at
/// live rows found nothing, and the insert then failed on a constraint the
/// caller had no way to see. Deleting a tag and typing its name again is
/// ordinary enough that it has to work.
#[tokio::test]
async fn a_deleted_tags_name_can_be_used_again() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let first = db.get_or_create_tag("recycled", None).await.unwrap();
    db.delete_tag(&first).await.unwrap();
    assert!(
        db.list_tags().await.unwrap().is_empty(),
        "the tag must be gone from the visible list"
    );

    let second = db
        .get_or_create_tag("recycled", None)
        .await
        .expect("creating a tag whose name a tombstone still holds must succeed");

    assert_eq!(
        second, first,
        "the dead row must be revived rather than duplicated: a fresh id would \
         leave a peer that still holds the tag with a second tag of the same name"
    );
    assert_eq!(
        db.list_tags().await.unwrap().len(),
        1,
        "the revived tag must be visible again"
    );
}

/// Stamp a row far enough ahead to stand in for a skewed peer.
async fn push_clock_forward(db: &Database, sql: &str) -> i64 {
    let future = chrono::Utc::now().timestamp_millis() + 86_400_000;
    db.conn()
        .execute(
            sql,
            turso::params::Params::Positional(vec![turso::Value::Integer(future)]),
        )
        .await
        .unwrap();
    future
}

/// Re-adding a membership must outrank a peer's future-stamped row.
///
/// `upsert_junction` assigned `updated_at` raw where `stamp_row` clamps, so a
/// device holding a row a peer had stamped ahead of its own clock wrote *below*
/// the clock the row already carried. The membership looked live locally while
/// already losing to the peer's tombstone, and the next merge buried it: the
/// user re-added a tag and watched it silently revert.
///
/// A few seconds of skew between two machines is ordinary, so this needs no
/// misconfiguration to happen.
#[tokio::test]
async fn re_adding_a_membership_outranks_a_skewed_peer() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "Tagged").await;
    let tag = db.get_or_create_tag("t", None).await.unwrap();

    db.add_tag_to_paper(&paper, &tag).await.unwrap();
    let future = push_clock_forward(
        &db,
        "UPDATE paper_tags SET updated_at = ?1, updated_by = 'peer', deleted = 1",
    )
    .await;

    db.add_tag_to_paper(&paper, &tag).await.unwrap();

    let after = scalar(&db, "SELECT updated_at FROM paper_tags").await;
    assert!(
        after > future,
        "re-adding the membership must outrank the peer's stamp ({after} <= {future}), \
         or the next merge overwrites it with the tombstone and the re-add reverts"
    );
    assert_eq!(
        scalar(&db, "SELECT deleted FROM paper_tags").await,
        0,
        "and it must be live"
    );
}

/// The delete cascade must outrank a peer's future-stamped children.
///
/// `tombstone_children` and `tombstone_citations` assigned `updated_at` raw for
/// the same reason, so deleting a paper whose children a peer had stamped ahead
/// left tombstones that lost to the very rows they were meant to retire — the
/// paper would come back one child at a time.
#[tokio::test]
async fn the_delete_cascade_outranks_skewed_children() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "Doomed").await;
    let other = insert_paper(&db, "Cited").await;

    let mut note = rotero_models::Note::new(paper.clone(), "n".into());
    note.body = "n".into();
    db.insert_note(&note).await.unwrap();
    db.insert_citation(&paper, &other).await.unwrap();

    let future =
        push_clock_forward(&db, "UPDATE notes SET updated_at = ?1, updated_by = 'peer'").await;
    push_clock_forward(
        &db,
        "UPDATE paper_citations SET updated_at = ?1, updated_by = 'peer'",
    )
    .await;

    db.delete_paper(&paper).await.unwrap();

    for (table, what) in [("notes", "a note"), ("paper_citations", "a citation edge")] {
        let at = scalar(&db, &format!("SELECT updated_at FROM {table}")).await;
        assert!(
            at > future,
            "the tombstone on {what} must outrank the peer's stamp ({at} <= {future}), \
             or the child survives the merge and resurrects the paper"
        );
        assert_eq!(
            scalar(&db, &format!("SELECT deleted FROM {table}")).await,
            1,
            "and {what} must be tombstoned"
        );
    }
}

//! The reaper is the only thing here that destroys data irreversibly.
//!
//! Every test below is about refusing to reap. Reaping too eagerly resurrects
//! deleted papers on the next merge — the failure this whole design exists to
//! avoid — so the safety conditions are asserted individually rather than
//! inferred from one happy path.

mod common;

use rotero_db::Database;
use rotero_db::reaper::TOMBSTONE_TTL_MS;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

async fn tombstone_count(db: &Database) -> i64 {
    let mut rows = db
        .conn()
        .query("SELECT COUNT(*) FROM papers WHERE deleted = 1", ())
        .await
        .unwrap();
    rows.next()
        .await
        .unwrap()
        .and_then(|r| r.get_value(0).ok())
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(-1)
}

/// Create a tombstone stamped `age_ms` in the past, attributed to `device`.
async fn aged_tombstone(db: &Database, title: &str, age_ms: i64, device: &str, now: i64) {
    let id = db
        .insert_paper(&rotero_models::Paper {
            title: title.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    db.delete_paper(&id).await.unwrap();
    db.conn()
        .execute(
            "UPDATE papers SET updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::Integer(now - age_ms),
                turso::Value::Text(device.to_string()),
                turso::Value::Text(id),
            ]),
        )
        .await
        .unwrap();
}

/// With no readable peer snapshot there is no evidence anyone saw the deletion.
#[tokio::test]
async fn nothing_is_reaped_without_a_peer_horizon() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();

    let device = db.device_id().to_string();
    aged_tombstone(&db, "Ancient", TOMBSTONE_TTL_MS * 10, &device, now).await;

    let stats = db.reap_tombstones(None, now).await.unwrap();
    assert_eq!(
        stats.removed, 0,
        "with no peer horizon, even an ancient tombstone must be kept"
    );
    assert_eq!(tombstone_count(&db).await, 1);
}

/// A peer that has not published recently holds everything back.
#[tokio::test]
async fn a_stale_peer_horizon_blocks_reaping() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();

    let device = db.device_id().to_string();
    aged_tombstone(&db, "Old", TOMBSTONE_TTL_MS + DAY_MS, &device, now).await;

    // The peer's newest snapshot predates the tombstone, so it cannot have seen
    // the deletion yet.
    let horizon = now - TOMBSTONE_TTL_MS * 3;
    let stats = db.reap_tombstones(Some(horizon), now).await.unwrap();

    assert_eq!(
        stats.removed, 0,
        "a peer that has not caught up must hold its tombstones back"
    );
}

/// A tombstone younger than the TTL is never reaped, however current the peers.
#[tokio::test]
async fn a_recent_tombstone_is_never_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();

    let device = db.device_id().to_string();
    aged_tombstone(&db, "Yesterday", DAY_MS, &device, now).await;

    let stats = db.reap_tombstones(Some(now), now).await.unwrap();
    assert_eq!(
        stats.removed, 0,
        "a fresh peer snapshot must not make recent deletions reapable"
    );
    assert_eq!(tombstone_count(&db).await, 1);
}

/// Another device's tombstone is never reaped locally.
#[tokio::test]
async fn a_peers_tombstone_is_left_alone() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();

    // Old enough on every other axis, but written by someone else.
    aged_tombstone(&db, "Theirs", TOMBSTONE_TTL_MS * 5, "someone-else", now).await;

    let stats = db
        .reap_tombstones(Some(now - TOMBSTONE_TTL_MS * 2), now)
        .await
        .unwrap();

    assert_eq!(
        stats.removed, 0,
        "only the device that wrote a tombstone may reap it; deleting a peer's \
         copy either achieves nothing or loses the deletion outright"
    );
    assert_eq!(tombstone_count(&db).await, 1);
}

/// A settled tombstone this device owns is removed.
#[tokio::test]
async fn a_settled_tombstone_is_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();

    let device = db.device_id().to_string();
    aged_tombstone(&db, "Long gone", TOMBSTONE_TTL_MS * 4, &device, now).await;
    assert_eq!(tombstone_count(&db).await, 1);

    // Every peer has published well after the deletion.
    let horizon = now - TOMBSTONE_TTL_MS;
    let stats = db.reap_tombstones(Some(horizon), now).await.unwrap();

    assert_eq!(stats.removed, 1, "a settled tombstone must be removed");
    assert_eq!(tombstone_count(&db).await, 0);
}

/// Live rows are never touched, however old.
#[tokio::test]
async fn live_rows_are_never_reaped() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();

    let id = db
        .insert_paper(&rotero_models::Paper {
            title: "Ancient but alive".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    db.conn()
        .execute(
            "UPDATE papers SET updated_at = ?1, updated_by = ?2 WHERE id = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::Integer(now - TOMBSTONE_TTL_MS * 10),
                turso::Value::Text(db.device_id().to_string()),
                turso::Value::Text(id),
            ]),
        )
        .await
        .unwrap();

    db.reap_tombstones(Some(now - TOMBSTONE_TTL_MS), now)
        .await
        .unwrap();

    assert_eq!(
        db.list_papers().await.unwrap().len(),
        1,
        "a live row must survive regardless of age"
    );
}

/// The reaper does not run on every launch.
#[tokio::test]
async fn reaping_is_rate_limited() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let now = chrono::Utc::now().timestamp_millis();
    let device = db.device_id().to_string();
    let horizon = now - TOMBSTONE_TTL_MS;

    aged_tombstone(&db, "First", TOMBSTONE_TTL_MS * 4, &device, now).await;
    assert_eq!(
        db.reap_tombstones(Some(horizon), now)
            .await
            .unwrap()
            .removed,
        1
    );

    // A second tombstone, and another launch an hour later.
    aged_tombstone(&db, "Second", TOMBSTONE_TTL_MS * 4, &device, now).await;
    let stats = db
        .reap_tombstones(Some(horizon), now + 60 * 60 * 1000)
        .await
        .unwrap();

    assert_eq!(stats.removed, 0, "a full scan must not run on every launch");
    assert_eq!(
        tombstone_count(&db).await,
        1,
        "the second is simply deferred"
    );
}

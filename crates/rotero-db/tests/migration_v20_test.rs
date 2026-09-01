//! The v20 migration gives `papers` a clock per column.
//!
//! Row-level LWW decided the whole row on one clock, so a peer row that lost
//! discarded every column — including ones it changed and this device did not.
//! Editing a title on one device while another attached a PDF silently lost one
//! of the two.
//!
//! Two things have to hold for that change to be safe on a library that already
//! exists: the migration must not alter what any existing row means, and a
//! device still running row-level LWW must keep syncing rather than silently stop.

mod common;

use rotero_db::snapshot::SnapshotRow;
use rotero_models::Paper;
use std::collections::BTreeMap;

/// Rewind so reopening runs the v20 block.
async fn rewind_to_v19(dir: &std::path::Path) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(19)],
    )
    .await
    .unwrap();
}

async fn clock_of(db: &rotero_db::Database, id: &str, column: &str) -> (i64, String) {
    let mut rows = db
        .conn()
        .query(
            &format!("SELECT {column}_ua, {column}_ub FROM papers WHERE id = ?1"),
            [turso::Value::Text(id.to_string())],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("the paper must exist");
    (
        row.get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0),
        row.get_value(1)
            .ok()
            .and_then(|v| v.as_text().cloned())
            .unwrap_or_default(),
    )
}

async fn row_clock(db: &rotero_db::Database, id: &str) -> (i64, String) {
    let mut rows = db
        .conn()
        .query(
            "SELECT updated_at, updated_by FROM papers WHERE id = ?1",
            [turso::Value::Text(id.to_string())],
        )
        .await
        .unwrap();
    let row = rows.next().await.unwrap().expect("the paper must exist");
    (
        row.get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0),
        row.get_value(1)
            .ok()
            .and_then(|v| v.as_text().cloned())
            .unwrap_or_default(),
    )
}

/// The backfill copies the row clock down onto every column.
///
/// Not zero. A column left at 0 loses every comparison, so any peer's stale
/// copy of it would win — the migration would hand every pre-existing field to
/// whichever device happened to speak next. Copying the row clock down makes
/// per-column comparison reproduce exactly the row-level outcome, so a library
/// that never syncs again behaves identically to before.
#[tokio::test]
async fn the_backfill_copies_the_row_clock_onto_every_column() {
    let dir = tempfile::tempdir().unwrap();

    let id = {
        let db = common::open_test_db(dir.path()).await;
        db.insert_paper(&Paper {
            title: "Existing".into(),
            ..Default::default()
        })
        .await
        .unwrap()
    };

    rewind_to_v19(dir.path()).await;

    // Wipe the clocks, as a library written before the column existed has them.
    {
        let raw = turso::Builder::new_local(dir.path().join("rotero.db").to_str().unwrap())
            .experimental_index_method(true)
            .build()
            .await
            .unwrap();
        let conn = raw.connect().unwrap();
        conn.execute("UPDATE papers SET title_ua = 0, title_ub = ''", ())
            .await
            .unwrap();
    }

    let db = common::open_test_db(dir.path()).await;

    let (row_at, row_by) = row_clock(&db, &id).await;
    let (col_at, col_by) = clock_of(&db, &id, "title").await;

    assert_eq!(
        (col_at, col_by.as_str()),
        (row_at, row_by.as_str()),
        "the backfill must copy the row clock down, not leave the column at 0"
    );
    assert!(row_at > 0, "the row itself must still be stamped");
}

/// A peer that sends no per-column clocks still merges.
///
/// A device running v18 publishes rows with no `_ua` keys at all. Reading a
/// missing clock as zero would make every column of that peer's rows lose every
/// comparison — a total, silent sync failure between the two versions, and the
/// worst outcome this change can produce. The fallback reads the row's own
/// clock instead, which is precisely what row-level LWW meant.
#[tokio::test]
async fn a_peer_without_column_clocks_still_merges() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let id = db
        .insert_paper(&Paper {
            title: "Local".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let (local_at, _) = row_clock(&db, &id).await;

    // A v18-shaped row: payload columns only, no `_ua`/`_ub` keys anywhere.
    let values = v18_values("From an old peer");

    let row = SnapshotRow {
        t: "papers".into(),
        k: vec![id.clone()],
        v: Some(values),
        ua: local_at + 5_000,
        ub: "old-peer".into(),
        d: false,
    };

    let bytes = common::sync_harness::encode_snapshot("old-peer", &[row]);
    db.merge_snapshot(&bytes).await.unwrap();

    let got = db
        .get_papers_by_ids(std::slice::from_ref(&id))
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        got.title, "From an old peer",
        "a peer with no column clocks must still be able to win — otherwise \
         a row-LWW peer and a per-column peer stop exchanging anything, silently"
    );

    // The fallback stamps the column with the row's clock, so the value and its
    // provenance stay consistent for the next comparison.
    let (col_at, col_by) = clock_of(&db, &id, "title").await;
    assert_eq!(
        (col_at, col_by.as_str()),
        (local_at + 5_000, "old-peer"),
        "the column clock must inherit the row clock the peer sent"
    );
}

/// An old peer's row still loses when it is genuinely older.
///
/// The counterpart to the test above: the fallback must not make a v18 peer win
/// unconditionally, only let it compete on the terms row-level LWW used.
#[tokio::test]
async fn an_older_peer_without_column_clocks_still_loses() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let id = db
        .insert_paper(&Paper {
            title: "Local wins".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let (local_at, _) = row_clock(&db, &id).await;

    let values = v18_values("Stale");

    let row = SnapshotRow {
        t: "papers".into(),
        k: vec![id.clone()],
        v: Some(values),
        ua: local_at - 5_000,
        ub: "old-peer".into(),
        d: false,
    };

    let bytes = common::sync_harness::encode_snapshot("old-peer", &[row]);
    db.merge_snapshot(&bytes).await.unwrap();

    let got = db
        .get_papers_by_ids(std::slice::from_ref(&id))
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        got.title, "Local wins",
        "an older peer must not win just because it sent no column clocks"
    );
}

/// A v18-shaped payload: every payload column, and no `_ua`/`_ub` keys.
///
/// Built from the manifest rather than listed, so a column added later is
/// carried here too — a partial row would fail the table's NOT NULL constraints
/// and test the constraint rather than the fallback.
fn v18_values(title: &str) -> BTreeMap<String, serde_json::Value> {
    let table = rotero_db::sync_schema::synced_table("papers").unwrap();
    let mut values = BTreeMap::new();
    for column in table.columns {
        let v = match *column {
            "title" => serde_json::json!(title),
            "authors" => serde_json::json!("[]"),
            "date_added" | "date_modified" => serde_json::json!("2026-01-01T00:00:00Z"),
            "item_type" => serde_json::json!("journalArticle"),
            "is_favorite" | "is_read" => serde_json::json!(0),
            _ => serde_json::Value::Null,
        };
        values.insert((*column).to_string(), v);
    }
    values
}

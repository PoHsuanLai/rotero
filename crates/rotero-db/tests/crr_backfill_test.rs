//! Repair for libraries written by a build that never initialized CRR.
//!
//! Those rows exist in the app tables with no clock entries, so `changes_since`
//! — which reads only from clock tables — never emits them and they stay on the
//! machine that created them. Adding `crr.init()` back creates the clock tables
//! but leaves them empty, so the rows still need to be adopted explicitly.

mod common;

use rotero_db::Database;

/// Build a library the way the broken build did: app tables, no CRR metadata,
/// with rows already written into it.
async fn broken_library_with_rows(dir: &std::path::Path, titles: &[&str]) {
    common::open_uninitialized_db(dir).await;

    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();

    for (i, title) in titles.iter().enumerate() {
        conn.execute(
            "INSERT INTO papers (id, title, authors, date_added, date_modified)
             VALUES (?1, ?2, '[]', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            [
                turso::Value::Text(format!("paper-{i}")),
                turso::Value::Text(title.to_string()),
            ],
        )
        .await
        .unwrap();
    }

    drop(conn);
    drop(raw);
}

/// The repair's whole purpose: rows written while tracking was broken must
/// become syncable. Asserted through `changes_since`, which is what a peer
/// actually reads — the presence of clock rows alone would not prove it.
#[tokio::test]
async fn untracked_rows_become_syncable() {
    let dir = tempfile::tempdir().unwrap();
    broken_library_with_rows(dir.path(), &["Orphan One", "Orphan Two"]).await;

    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    let changes = db.crr().changes_since(0).await.expect("changes_since");
    let pks: std::collections::HashSet<&str> = changes
        .iter()
        .filter(|c| c.table_name == "papers")
        .map(|c| c.pk.as_str())
        .collect();

    assert!(
        pks.contains("paper-0") && pks.contains("paper-1"),
        "both orphaned rows must be emitted to peers, got {pks:?}"
    );
}

/// The scan is gated on a persisted flag, so a healthy library pays for it once.
/// Without the gate every launch would re-scan every table, and — worse — an
/// adopted row could be re-versioned on each open.
#[tokio::test]
async fn backfill_runs_only_once() {
    let dir = tempfile::tempdir().unwrap();
    broken_library_with_rows(dir.path(), &["Orphan"]).await;

    let first = {
        let db = Database::open(dir.path().to_path_buf()).await.unwrap();
        db.crr().changes_since(0).await.unwrap().len()
    };

    let second = {
        let db = Database::open(dir.path().to_path_buf()).await.unwrap();
        db.crr().changes_since(0).await.unwrap().len()
    };

    assert_eq!(
        first, second,
        "reopening must not re-adopt rows or emit further changes"
    );
}

/// Rows inserted normally already have clock entries, and adopting them again
/// would reset their versions — silently discarding edits a peer had not yet
/// seen.
///
/// The assertion is on `col_ver`, which is the *only* observable that can catch
/// this. An earlier version of this test compared `changes_since().len()` and the
/// local title, and passed even with the backfill's guards removed: re-adopting
/// an existing clock row adds no change rows and alters no local data, it only
/// rewinds the version. The damage is a cross-device LWW outcome, so a
/// single-database row count cannot see it.
#[tokio::test]
async fn already_tracked_rows_are_not_touched() {
    let dir = tempfile::tempdir().unwrap();

    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let paper = rotero_models::Paper {
        title: "Tracked".into(),
        ..Default::default()
    };
    let id = db.insert_paper(&paper).await.unwrap();

    // An edit lifts the title's version above the col_ver = 1 a backfill writes.
    let edited = rotero_models::Paper {
        title: "Edited".into(),
        ..paper.clone()
    };
    db.update_paper_metadata(&id, &edited).await.unwrap();

    let before = common::col_ver(&db, "papers", &id, "title").await;
    assert!(
        before >= 2,
        "an edited column must be past the col_ver a backfill writes, got {before}"
    );
    drop(db);

    // Clear the flag so the scan genuinely runs against this row. Without this
    // the reopen short-circuits at the gate and the test proves nothing about
    // adoption — it would pass even if adoption rewound every row it touched.
    common::clear_backfill_flags(dir.path()).await;

    let db = Database::open(dir.path().to_path_buf()).await.unwrap();
    let after = common::col_ver(&db, "papers", &id, "title").await;
    assert_eq!(
        after, before,
        "adoption must not rewind an already-tracked row's column version"
    );

    let papers = db.list_papers().await.unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(
        papers[0].title, "Edited",
        "the edit must survive; a re-adopted row would revert it"
    );
}

/// The repair must adopt a row that was deleted and then re-created while
/// tracking was broken.
///
/// `track_delete` bumps the sentinel to an even value and leaves it in place, so
/// such a row has a clock entry that reads *deleted*. Selecting only rows with no
/// clock entry therefore skips it — and since the flag is then stamped, it is
/// skipped forever. Locally the tag stays visible, so nothing looks wrong; the
/// damage only shows on another device, which is where this asserts.
#[tokio::test]
async fn a_tag_removed_and_re_added_while_broken_still_syncs() {
    let shared = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();

    let paper_id;
    let tag_id;
    {
        let db = Database::open(dir_a.path().to_path_buf()).await.unwrap();
        paper_id = db
            .insert_paper(&rotero_models::Paper {
                title: "Paper".into(),
                ..Default::default()
            })
            .await
            .unwrap();
        tag_id = db.get_or_create_tag("important", None).await.unwrap();

        // Attach and detach normally: the sentinel is now even (deleted).
        db.add_tag_to_paper(&paper_id, &tag_id).await.unwrap();
        db.remove_tag_from_paper(&paper_id, &tag_id).await.unwrap();
    }

    // The broken build re-attaches: the row commits, tracking never runs.
    common::insert_untracked_paper_tag(dir_a.path(), &paper_id, &tag_id).await;

    // Clear the flag so the repair re-runs, as the v1 -> v2 key bump does for a
    // library that was already stamped by the version with this bug.
    common::clear_backfill_flags(dir_a.path()).await;

    let db_a = Database::open(dir_a.path().to_path_buf()).await.unwrap();
    assert_eq!(
        db_a.list_tags_for_paper(&paper_id).await.unwrap().len(),
        1,
        "the tag is attached locally either way — this is why the bug hid"
    );

    let dir_b = tempfile::tempdir().unwrap();
    let db_b = Database::open(dir_b.path().to_path_buf()).await.unwrap();

    let engine_a =
        rotero_db::sync_test_helpers::TestSyncEngine::new(shared.path().to_path_buf(), vec![1; 16]);
    let engine_b =
        rotero_db::sync_test_helpers::TestSyncEngine::new(shared.path().to_path_buf(), vec![2; 16]);
    engine_a.export_changes(&db_a).await;
    engine_b.import_changes(&db_b).await;

    assert_eq!(
        db_b.list_tags_for_paper(&paper_id).await.unwrap().len(),
        1,
        "the re-added tag must reach the second device, not arrive as a delete"
    );
}

/// End to end: a repaired library's rows reach another device.
///
/// `changes_since` emitting a row is necessary but not sufficient — the change
/// still has to carry enough for a peer to materialize the row. This is the
/// assertion that the user-visible problem ("my papers never showed up on my
/// other machine") is actually fixed.
#[tokio::test]
async fn repaired_rows_reach_a_second_device() {
    let shared = tempfile::tempdir().unwrap();

    let dir_a = tempfile::tempdir().unwrap();
    broken_library_with_rows(dir_a.path(), &["Orphan One", "Orphan Two"]).await;
    let db_a = Database::open(dir_a.path().to_path_buf()).await.unwrap();

    let dir_b = tempfile::tempdir().unwrap();
    let db_b = Database::open(dir_b.path().to_path_buf()).await.unwrap();

    let engine_a =
        rotero_db::sync_test_helpers::TestSyncEngine::new(shared.path().to_path_buf(), vec![1; 16]);
    let engine_b =
        rotero_db::sync_test_helpers::TestSyncEngine::new(shared.path().to_path_buf(), vec![2; 16]);

    let exported = engine_a.export_changes(&db_a).await;
    assert!(
        exported > 0,
        "the repaired library must have changes to send"
    );
    engine_b.import_changes(&db_b).await;

    let mut titles: Vec<String> = db_b
        .list_papers()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.title)
        .collect();
    titles.sort();

    assert_eq!(
        titles,
        vec!["Orphan One".to_string(), "Orphan Two".to_string()],
        "rows written while tracking was broken must reach the second device"
    );
}

/// Junction tables key on two columns joined by a separator, so they take a
/// different query path than `id`-keyed tables. A tag attached while tracking
/// was broken is exactly the "my tags vanished" report, so it has to survive the
/// repair and sync like any other row.
#[tokio::test]
async fn composite_key_rows_are_adopted() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rotero.db");

    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    rotero_db::schema::initialize_db(&conn).await.unwrap();
    conn.execute(
        "INSERT INTO papers (id, title, authors, date_added, date_modified)
         VALUES ('p1', 'Paper', '[]', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
        (),
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO tags (id, name) VALUES ('t1', 'important')", ())
        .await
        .unwrap();
    conn.execute(
        "INSERT INTO paper_tags (paper_id, tag_id) VALUES ('p1', 't1')",
        (),
    )
    .await
    .unwrap();
    drop(conn);
    drop(raw);

    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    let changes = db.crr().changes_since(0).await.unwrap();
    let junction: Vec<&str> = changes
        .iter()
        .filter(|c| c.table_name == "paper_tags")
        .map(|c| c.pk.as_str())
        .collect();

    assert!(
        junction.contains(&"p1:t1"),
        "the composite key must be emitted joined by the schema's separator, got {junction:?}"
    );

    // And it still reads back as a real attachment locally.
    let tags = db.list_tags_for_paper("p1").await.unwrap();
    assert_eq!(tags.len(), 1);
}

/// A fresh library has nothing to adopt. Guards against the scan mistaking
/// normal empty tables for damage.
#[tokio::test]
async fn a_fresh_library_needs_no_repair() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    let changes = db.crr().changes_since(0).await.unwrap();
    assert!(
        changes.is_empty(),
        "a fresh library must emit no changes, got {changes:?}"
    );
}

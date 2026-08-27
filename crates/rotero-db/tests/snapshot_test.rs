//! The snapshot format and the merge that consumes it.
//!
//! These are the correctness core of per-device sync: if the merge is not
//! idempotent and order-independent, devices diverge silently and no amount of
//! transport care recovers it.

mod common;

use rotero_db::Database;

async fn insert_paper(db: &Database, title: &str) -> String {
    db.insert_paper(&rotero_models::Paper {
        title: title.into(),
        ..Default::default()
    })
    .await
    .unwrap()
}

async fn titles(db: &Database) -> Vec<String> {
    let mut t: Vec<String> = db
        .list_papers()
        .await
        .unwrap()
        .into_iter()
        .map(|p| p.title)
        .collect();
    t.sort();
    t
}

/// A snapshot survives a round trip.
#[tokio::test]
async fn snapshot_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    insert_paper(&db, "Round trip").await;

    let bytes = db.write_snapshot().await.unwrap();
    let (header, rows) = rotero_db::snapshot::parse_snapshot(&bytes).unwrap();

    assert_eq!(header.site_id, db.device_id());
    assert_eq!(header.rows, rows.len(), "header must match its body");
    assert!(
        rows.iter().any(|r| r.t == "papers"),
        "the paper must be in the snapshot"
    );
}

/// A truncated file is rejected rather than half-applied.
#[tokio::test]
async fn truncation_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    for i in 0..5 {
        insert_paper(&db, &format!("Paper {i}")).await;
    }

    let bytes = db.write_snapshot().await.unwrap();
    let cut = &bytes[..bytes.len() * 2 / 3];

    assert!(
        rotero_db::snapshot::parse_snapshot(cut).is_err(),
        "a truncated snapshot must fail to parse, not yield half a library"
    );
}

/// A snapshot from a newer build is refused, not misread.
#[tokio::test]
async fn a_newer_format_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    insert_paper(&db, "Future").await;

    let bytes = db.write_snapshot().await.unwrap();
    let (_, rows) = rotero_db::snapshot::parse_snapshot(&bytes).unwrap();

    // Rebuild with a bumped format version.
    let mut plain = format!(
        "{}\n",
        serde_json::json!({
            "format": 999,
            "site_id": "peer",
            "generated_at": 0,
            "rows": rows.len(),
        })
    );
    for row in &rows {
        plain.push_str(&serde_json::to_string(row).unwrap());
        plain.push('\n');
    }
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut enc, plain.as_bytes()).unwrap();
    let bumped = enc.finish().unwrap();

    assert!(
        matches!(
            rotero_db::snapshot::parse_snapshot(&bumped),
            Err(rotero_db::snapshot::SnapshotError::NewerFormat { .. })
        ),
        "a newer format must be refused by name, not silently misread"
    );
}

/// A newer peer row wins; an older one does not.
#[tokio::test]
async fn merge_resolves_by_clock() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = common::open_test_db(dir_a.path()).await;
    let b = common::open_test_db(dir_b.path()).await;

    let paper = insert_paper(&a, "From A").await;
    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();
    assert_eq!(titles(&b).await, vec!["From A"]);

    // B edits later, so B wins.
    b.update_paper_metadata(
        &paper,
        &rotero_models::Paper {
            title: "Edited on B".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    a.merge_snapshot(&b.write_snapshot().await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        titles(&a).await,
        vec!["Edited on B"],
        "the newer edit must win"
    );

    // Re-merging A's now-older snapshot must not undo it.
    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();
    assert_eq!(
        titles(&b).await,
        vec!["Edited on B"],
        "an older peer row must not clobber a newer local one"
    );
}

/// Merging the same snapshot twice changes nothing the second time.
#[tokio::test]
async fn merge_is_idempotent() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = common::open_test_db(dir_a.path()).await;
    let b = common::open_test_db(dir_b.path()).await;

    insert_paper(&a, "Once").await;
    let snap = a.write_snapshot().await.unwrap();

    let first = b.merge_snapshot(&snap).await.unwrap();
    let second = b.merge_snapshot(&snap).await.unwrap();

    assert!(first.applied > 0, "the first merge must apply something");
    assert_eq!(
        second.applied, 0,
        "re-applying the same snapshot must be a no-op"
    );
    assert_eq!(titles(&b).await, vec!["Once"]);
}

/// A deletion reaches the other device.
#[tokio::test]
async fn tombstones_propagate() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = common::open_test_db(dir_a.path()).await;
    let b = common::open_test_db(dir_b.path()).await;

    let paper = insert_paper(&a, "Doomed").await;
    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();
    assert_eq!(titles(&b).await.len(), 1);

    a.delete_paper(&paper).await.unwrap();
    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();

    assert!(
        titles(&b).await.is_empty(),
        "a deletion must reach the second device, not just the first"
    );
}

/// Three devices converge regardless of the order they exchange snapshots.
#[tokio::test]
async fn three_devices_converge_in_any_order() {
    async fn run(order: &[(usize, usize)]) -> Vec<Vec<String>> {
        let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
        let mut dbs = Vec::new();
        for d in &dirs {
            dbs.push(common::open_test_db(d.path()).await);
        }

        for (i, db) in dbs.iter().enumerate() {
            insert_paper(db, &format!("From {i}")).await;
        }

        // Exchange to a fixpoint in the given order.
        for _ in 0..3 {
            for (from, to) in order {
                let snap = dbs[*from].write_snapshot().await.unwrap();
                dbs[*to].merge_snapshot(&snap).await.unwrap();
            }
        }

        let mut out = Vec::new();
        for db in &dbs {
            out.push(titles(db).await);
        }
        out
    }

    let forward = run(&[(0, 1), (1, 2), (2, 0), (0, 2), (2, 1), (1, 0)]).await;
    let reverse = run(&[(1, 0), (2, 1), (0, 2), (2, 0), (1, 2), (0, 1)]).await;

    let expected = vec![
        "From 0".to_string(),
        "From 1".to_string(),
        "From 2".to_string(),
    ];
    for (i, got) in forward.iter().enumerate() {
        assert_eq!(got, &expected, "device {i} diverged (forward order)");
    }
    for (i, got) in reverse.iter().enumerate() {
        assert_eq!(
            got, &expected,
            "device {i} reached a different state under a different exchange order"
        );
    }
}

/// A device must ignore its own snapshot, even a stale one.
///
/// Asserting only that re-merging applies nothing proves little: the clock guard
/// rejects an equal-timestamp row anyway. The check has to bite on a snapshot
/// that *would* otherwise apply — an older copy of this device's own state,
/// which is what a cloud folder serves up after a rollback or a slow sync.
#[tokio::test]
async fn a_device_ignores_its_own_stale_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "Original").await;

    let stale = db.write_snapshot().await.unwrap();

    db.update_paper_metadata(
        &paper,
        &rotero_models::Paper {
            title: "Edited since".into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let stats = db.merge_snapshot(&stale).await.unwrap();

    assert_eq!(stats.applied, 0, "a device must not merge its own snapshot");
    assert_eq!(
        titles(&db).await,
        vec!["Edited since"],
        "its own stale snapshot must not roll the library back"
    );
}

/// An equal timestamp is broken by device id, the same way on every device.
///
/// Without this the merge is not deterministic: two devices that stamped the
/// same millisecond would each keep their own copy and stay diverged forever,
/// with nothing in the data to indicate which is right.
#[tokio::test]
async fn an_equal_timestamp_is_broken_by_device_id() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = common::open_test_db(dir_a.path()).await;
    let b = common::open_test_db(dir_b.path()).await;

    let paper = insert_paper(&a, "Shared").await;
    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();

    // Force both devices to the same instant with distinguishable ids, which is
    // otherwise vanishingly rare and so never covered.
    let now = chrono::Utc::now().timestamp_millis() + 60_000;
    for (db, device, title) in [(&a, "aaaa", "From A"), (&b, "bbbb", "From B")] {
        db.conn()
            .execute(
                "UPDATE papers SET title = ?1, updated_at = ?2, updated_by = ?3 WHERE id = ?4",
                turso::params::Params::Positional(vec![
                    turso::Value::Text(title.into()),
                    turso::Value::Integer(now),
                    turso::Value::Text(device.into()),
                    turso::Value::Text(paper.clone()),
                ]),
            )
            .await
            .unwrap();
    }

    // Exchange both ways; the higher device id must win on both.
    a.merge_snapshot(&b.write_snapshot().await.unwrap())
        .await
        .unwrap();
    b.merge_snapshot(&a.write_snapshot().await.unwrap())
        .await
        .unwrap();

    assert_eq!(
        titles(&a).await,
        vec!["From B"],
        "the higher device id must win the tie"
    );
    assert_eq!(
        titles(&b).await,
        titles(&a).await,
        "and both devices must reach the same answer, or they stay diverged"
    );
}

/// A tombstone carries no payload.
///
/// Deletions are the bulk of what a long-lived library accumulates, and a
/// tombstone needs only its key and clock. Shipping the dead row's values would
/// grow every snapshot for nothing.
#[tokio::test]
async fn a_tombstone_carries_no_payload() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "Doomed").await;
    db.delete_paper(&paper).await.unwrap();

    let bytes = db.write_snapshot().await.unwrap();
    let (_, rows) = rotero_db::snapshot::parse_snapshot(&bytes).unwrap();

    let row = rows
        .iter()
        .find(|r| r.t == "papers" && r.k[0] == paper)
        .expect("the tombstone must be in the snapshot");

    assert!(row.d, "the row must be marked deleted");
    assert!(
        row.v.is_none(),
        "a tombstone must carry no payload, only its key and clock"
    );
}

/// A snapshot with fewer rows than its header promises is rejected.
///
/// Distinct from a truncated gzip stream, which fails at decompression. A file
/// that decompresses cleanly but stops early is what a partly-flushed write
/// leaves behind, and it would otherwise apply as a smaller library.
#[tokio::test]
async fn a_short_snapshot_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;
    for i in 0..5 {
        insert_paper(&db, &format!("Paper {i}")).await;
    }

    let bytes = db.write_snapshot().await.unwrap();
    let mut plain = String::new();
    std::io::Read::read_to_string(&mut flate2::read::GzDecoder::new(&bytes[..]), &mut plain)
        .unwrap();

    // Keep the header, drop the last row, and re-compress cleanly.
    let mut lines: Vec<&str> = plain.lines().collect();
    lines.pop();
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut enc, (lines.join("\n") + "\n").as_bytes()).unwrap();
    let short = enc.finish().unwrap();

    let err = rotero_db::snapshot::parse_snapshot(&short)
        .expect_err("a snapshot shorter than its header claims must be rejected");
    assert!(
        format!("{err}").contains("truncated"),
        "the error should name the problem, got: {err}"
    );
}

/// Two devices creating the same tag name offline converge on one tag.
///
/// `tags.name` is UNIQUE, so the second insert would otherwise fail and the
/// merge would stall on every subsequent sync.
#[tokio::test]
async fn same_tag_name_on_two_devices_converges() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let a = common::open_test_db(dir_a.path()).await;
    let b = common::open_test_db(dir_b.path()).await;

    // Each device independently creates "ml" and tags its own paper with it.
    let paper_a = insert_paper(&a, "On A").await;
    let paper_b = insert_paper(&b, "On B").await;
    let tag_a = a.get_or_create_tag("ml", None).await.unwrap();
    let tag_b = b.get_or_create_tag("ml", None).await.unwrap();
    assert_ne!(tag_a, tag_b, "the two tags must start with different ids");

    a.add_tag_to_paper(&paper_a, &tag_a).await.unwrap();
    b.add_tag_to_paper(&paper_b, &tag_b).await.unwrap();

    // Exchange to a fixpoint.
    for _ in 0..3 {
        let snap_a = a.write_snapshot().await.unwrap();
        b.merge_snapshot(&snap_a).await.unwrap();
        let snap_b = b.write_snapshot().await.unwrap();
        a.merge_snapshot(&snap_b).await.unwrap();
    }

    for (name, db) in [("A", &a), ("B", &b)] {
        let tags = db.list_tags().await.unwrap();
        assert_eq!(
            tags.len(),
            1,
            "device {name} kept {} tags named \"ml\" instead of one",
            tags.len()
        );
    }

    let survivor_a = a.list_tags().await.unwrap()[0].id.clone();
    let survivor_b = b.list_tags().await.unwrap()[0].id.clone();
    assert_eq!(
        survivor_a, survivor_b,
        "both devices must pick the same survivor, or they have not converged"
    );

    // The paper tagged on each device must still carry the surviving tag.
    for (name, db, paper) in [("A", &a, &paper_a), ("B", &b, &paper_b)] {
        assert_eq!(
            db.list_tags_for_paper(paper).await.unwrap().len(),
            1,
            "device {name} lost the membership when the duplicate tag was retired"
        );
    }
}

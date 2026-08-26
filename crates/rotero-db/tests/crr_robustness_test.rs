//! Robustness tests for CRR sync — edge cases, concurrent mutations,
//! idempotency, out-of-order application, delete/resurrect, junction tables.

use rotero_db::Database;
use rotero_db::crr::ChangeRow;
use rotero_models::{Annotation, AnnotationType, Collection, Note, Paper};

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

fn new_paper(title: &str) -> Paper {
    Paper::new(title.to_string())
}

/// Helper: set up two DBs with the same paper (same UUID), both with clocks.
async fn setup_two_devices_same_paper(
    dir_a: &std::path::Path,
    dir_b: &std::path::Path,
) -> (Database, Database, String) {
    let db_a = open_test_db(dir_a).await;
    let db_b = open_test_db(dir_b).await;

    let id = db_a.insert_paper(&new_paper("Shared Paper")).await.unwrap();

    // Replicate to B via sync
    let changes = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes).await.unwrap();

    // Verify B has it
    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].id.as_deref(), Some(id.as_str()));

    (db_a, db_b, id)
}

// ── Delete vs Edit conflict ─────────────────────────────────────

#[tokio::test]
async fn test_delete_on_a_edit_on_b_delete_wins() {
    // Delete should win over edit because CL increases (delete CL=2 > edit CL=1)
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (db_a, db_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A deletes the paper
    db_a.delete_paper(&id).await.unwrap();

    // B edits the paper (doesn't know about the delete yet)
    db_b.set_favorite(&id, true).await.unwrap();

    // Sync A's changes to B
    let changes_a = db_a.crr().changes_since(0).await.unwrap();
    let result = db_b.apply_changes(&changes_a).await.unwrap();
    assert!(result.applied > 0);

    // Paper should be deleted on B (delete CL=2 beats alive CL=1)
    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 0, "Delete should win over edit");
}

#[tokio::test]
async fn test_edit_on_a_delete_on_b_delete_wins() {
    // Same but reversed — B deletes, A edits
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (db_a, db_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A edits
    db_a.set_read(&id, true).await.unwrap();

    // B deletes
    db_b.delete_paper(&id).await.unwrap();

    // Sync B's changes to A
    let changes_b = db_b.crr().changes_since(0).await.unwrap();
    db_a.apply_changes(&changes_b).await.unwrap();

    let papers_a = db_a.list_papers().await.unwrap();
    assert_eq!(papers_a.len(), 0, "Delete should win over edit");
}

// ── Idempotency ─────────────────────────────────────────────────

#[tokio::test]
async fn test_apply_same_changeset_twice_is_idempotent() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // A inserts and updates
    let id = db_a
        .insert_paper(&new_paper("Idempotent Paper"))
        .await
        .unwrap();
    db_a.set_favorite(&id, true).await.unwrap();

    let changes = db_a.crr().changes_since(0).await.unwrap();

    // Apply to B twice
    let result1 = db_b.apply_changes(&changes).await.unwrap();
    let result2 = db_b.apply_changes(&changes).await.unwrap();

    // Second application should skip everything
    assert!(result1.applied > 0);
    assert_eq!(result2.applied, 0, "Second application should be all skips");
    assert!(result2.skipped > 0);

    // Data should be correct
    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].title, "Idempotent Paper");
    assert!(papers_b[0].status.is_favorite);
}

// ── Out-of-order changeset application ──────────────────────────

/// Note: changesets from a single device must be applied in order.
/// changes_since() captures the current col_ver, not a historical snapshot,
/// so splitting a single device's changes into sub-batches and reordering
/// is not supported. This is fine in practice since both file sync and
/// CloudKit deliver changesets chronologically.
#[tokio::test]
async fn test_sequential_changesets_from_same_device() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // A: insert → update title → update favorite
    let id = db_a.insert_paper(&new_paper("Step 1")).await.unwrap();
    let v1 = db_a.crr().current_db_version().await.unwrap();

    let paper = new_paper("Step 2");
    db_a.update_paper_metadata(&id, &paper).await.unwrap();
    db_a.crr().current_db_version().await.unwrap();

    db_a.set_favorite(&id, true).await.unwrap();

    // Export as two sequential batches (as real sync would)
    let batch1 = db_a
        .crr()
        .changes_since(0)
        .await
        .unwrap()
        .into_iter()
        .filter(|c| c.db_ver <= v1)
        .collect::<Vec<_>>();
    let batch2 = db_a.crr().changes_since(v1).await.unwrap();

    // Apply in correct order
    db_b.apply_changes(&batch1).await.unwrap();
    db_b.apply_changes(&batch2).await.unwrap();

    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].title, "Step 2");
    assert!(papers_b[0].status.is_favorite);
}

// ── Multiple columns edited independently ───────────────────────

#[tokio::test]
async fn test_different_columns_merge_independently() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (db_a, db_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A changes favorite
    db_a.set_favorite(&id, true).await.unwrap();

    // B changes read status
    db_b.set_read(&id, true).await.unwrap();

    // Sync both ways
    let changes_a = db_a.crr().changes_since(0).await.unwrap();
    let changes_b = db_b.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes_a).await.unwrap();
    db_a.apply_changes(&changes_b).await.unwrap();

    // Both should have favorite=true AND read=true
    let papers_a = db_a.list_papers().await.unwrap();
    let papers_b = db_b.list_papers().await.unwrap();

    assert!(
        papers_a[0].status.is_favorite,
        "A should have favorite from A"
    );
    assert!(papers_a[0].status.is_read, "A should have read from B");
    assert!(
        papers_b[0].status.is_favorite,
        "B should have favorite from A"
    );
    assert!(papers_b[0].status.is_read, "B should have read from B");
}

// ── Convergence: both devices end up identical ──────────────────

#[tokio::test]
async fn test_bidirectional_sync_converges() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (db_a, db_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A: favorite + update title
    db_a.set_favorite(&id, true).await.unwrap();
    let paper_a = new_paper("Title A");
    db_a.update_paper_metadata(&id, &paper_a).await.unwrap();

    // B: read + different title
    db_b.set_read(&id, true).await.unwrap();
    let paper_b = new_paper("Title B");
    db_b.update_paper_metadata(&id, &paper_b).await.unwrap();

    // Round 1: sync A→B, B→A
    let changes_a = db_a.crr().changes_since(0).await.unwrap();
    let changes_b = db_b.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes_a).await.unwrap();
    db_a.apply_changes(&changes_b).await.unwrap();

    // Both should converge to the same state
    let pa = db_a.list_papers().await.unwrap();
    let pb = db_b.list_papers().await.unwrap();

    assert_eq!(pa[0].title, pb[0].title, "Titles should converge");
    assert_eq!(
        pa[0].status.is_favorite, pb[0].status.is_favorite,
        "Favorites should converge"
    );
    assert_eq!(
        pa[0].status.is_read, pb[0].status.is_read,
        "Read status should converge"
    );
}

// ── Junction tables ─────────────────────────────────────────────

#[tokio::test]
async fn test_junction_table_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // A: create paper, collection, add paper to collection
    let paper_id = db_a
        .insert_paper(&new_paper("Junction Test"))
        .await
        .unwrap();
    let coll = Collection::new("Test Collection".to_string());
    let coll_id = db_a.insert_collection(&coll).await.unwrap();
    db_a.add_paper_to_collection(&paper_id, &coll_id)
        .await
        .unwrap();

    // Sync to B
    let changes = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes).await.unwrap();

    // Verify B has the paper in the collection
    let ids_b = db_b.list_paper_ids_in_collection(&coll_id).await.unwrap();
    assert_eq!(ids_b.len(), 1);
    assert_eq!(ids_b[0], paper_id);
}

#[tokio::test]
async fn test_tag_junction_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // A: create paper, tag, add tag to paper
    let paper_id = db_a.insert_paper(&new_paper("Tagged Paper")).await.unwrap();
    let tag_id = db_a
        .get_or_create_tag("machine-learning", None)
        .await
        .unwrap();
    db_a.add_tag_to_paper(&paper_id, &tag_id).await.unwrap();

    // Sync to B
    let changes = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes).await.unwrap();

    // Verify B has the tag and paper-tag association
    let tags_b = db_b.list_tags().await.unwrap();
    assert!(tags_b.iter().any(|t| t.name == "machine-learning"));

    let tag_papers = db_b.list_paper_ids_by_tag(&tag_id).await.unwrap();
    assert_eq!(tag_papers.len(), 1);
    assert_eq!(tag_papers[0], paper_id);
}

// ── Annotations sync ────────────────────────────────────────────

#[tokio::test]
async fn test_annotation_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // A: create paper + annotation
    let paper_id = db_a
        .insert_paper(&new_paper("Annotated Paper"))
        .await
        .unwrap();
    let ann = Annotation {
        id: None,
        paper_id: paper_id.clone(),
        page: 1,
        ann_type: AnnotationType::Highlight,
        color: "#ffff00".to_string(),
        content: Some("Important finding".to_string()),
        geometry: serde_json::json!({"x": 10, "y": 20, "w": 100, "h": 15}),
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
    };
    db_a.insert_annotation(&ann).await.unwrap();

    // Sync to B
    let changes = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes).await.unwrap();

    // Verify B has the annotation
    let anns_b = db_b.list_annotations_for_paper(&paper_id).await.unwrap();
    assert_eq!(anns_b.len(), 1);
    assert_eq!(anns_b[0].content.as_deref(), Some("Important finding"));
    assert_eq!(anns_b[0].color, "#ffff00");
}

// ── Notes sync ──────────────────────────────────────────────────

#[tokio::test]
async fn test_notes_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    let paper_id = db_a
        .insert_paper(&new_paper("Paper with Notes"))
        .await
        .unwrap();
    let note = Note::new(paper_id.clone(), "My Note".to_string());
    db_a.insert_note(&note).await.unwrap();

    // Sync to B
    let changes = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes).await.unwrap();

    let notes_b = db_b.list_notes_for_paper(&paper_id).await.unwrap();
    assert_eq!(notes_b.len(), 1);
    assert_eq!(notes_b[0].title, "My Note");
}

// ── Bulk operations ─────────────────────────────────────────────

#[tokio::test]
async fn test_bulk_sync_100_papers() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // A: insert 100 papers
    for i in 0..100 {
        db_a.insert_paper(&new_paper(&format!("Paper {i}")))
            .await
            .unwrap();
    }

    // Sync to B
    let changes = db_a.crr().changes_since(0).await.unwrap();
    assert!(
        changes.len() > 100,
        "Should have many changes for 100 papers"
    );

    let result = db_b.apply_changes(&changes).await.unwrap();
    assert!(result.applied > 0);

    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 100, "B should have all 100 papers");
}

// ── Three-device convergence ────────────────────────────────────

#[tokio::test]
async fn test_three_device_convergence() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let dir_c = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;
    let db_c = open_test_db(dir_c.path()).await;

    // A creates a paper
    let id = db_a.insert_paper(&new_paper("Three Way")).await.unwrap();

    // Sync A→B and A→C
    let changes_a = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes_a).await.unwrap();
    db_c.apply_changes(&changes_a).await.unwrap();

    // Each device makes a different change
    db_a.set_favorite(&id, true).await.unwrap();
    db_b.set_read(&id, true).await.unwrap();
    let paper_c = new_paper("Updated by C");
    db_c.update_paper_metadata(&id, &paper_c).await.unwrap();

    // Gather all changes
    let ca = db_a.crr().changes_since(0).await.unwrap();
    let cb = db_b.crr().changes_since(0).await.unwrap();
    let cc = db_c.crr().changes_since(0).await.unwrap();

    // Apply all to all (full mesh sync)
    db_a.apply_changes(&cb).await.unwrap();
    db_a.apply_changes(&cc).await.unwrap();
    db_b.apply_changes(&ca).await.unwrap();
    db_b.apply_changes(&cc).await.unwrap();
    db_c.apply_changes(&ca).await.unwrap();
    db_c.apply_changes(&cb).await.unwrap();

    // All three should converge
    let pa = db_a.list_papers().await.unwrap();
    let pb = db_b.list_papers().await.unwrap();
    let pc = db_c.list_papers().await.unwrap();

    assert_eq!(pa[0].title, pb[0].title);
    assert_eq!(pb[0].title, pc[0].title);
    assert_eq!(pa[0].status.is_favorite, pb[0].status.is_favorite);
    assert_eq!(pa[0].status.is_read, pb[0].status.is_read);
    assert_eq!(pb[0].status.is_favorite, pc[0].status.is_favorite);
    assert_eq!(pb[0].status.is_read, pc[0].status.is_read);

    // All should have favorite=true and read=true
    assert!(pa[0].status.is_favorite);
    assert!(pa[0].status.is_read);
}

// ── Saved search sync ───────────────────────────────────────────

#[tokio::test]
async fn test_saved_search_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    let search =
        rotero_models::SavedSearch::new("ML papers".to_string(), "machine learning".to_string());
    db_a.insert_saved_search(&search).await.unwrap();

    let changes = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes).await.unwrap();

    let searches_b = db_b.list_saved_searches().await.unwrap();
    assert_eq!(searches_b.len(), 1);
    assert_eq!(searches_b[0].name, "ML papers");
    assert_eq!(searches_b[0].query, "machine learning");
}

// ── Resurrection ────────────────────────────────────────────────

#[tokio::test]
async fn test_resurrect_after_delete() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (db_a, db_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A deletes the paper (CL=2)
    db_a.delete_paper(&id).await.unwrap();

    // Sync A→B: B now has CL=2, paper deleted
    let changes_a = db_a.crr().changes_since(0).await.unwrap();
    db_b.apply_changes(&changes_a).await.unwrap();
    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 0, "Paper should be deleted after sync");

    // B resurrects: construct a changeset with CL=3 (odd = alive)
    // This simulates B explicitly re-creating the row after seeing the delete.
    let resurrect_changes = vec![
        ChangeRow {
            table_name: "papers".to_string(),
            pk: id.clone(),
            col_name: "__sentinel".to_string(),
            col_val: serde_json::Value::Null,
            col_ver: 3, // CL=3 (alive, after delete CL=2)
            db_ver: 999,
            site_id: db_b.crr().site_id().await.unwrap(),
            seq: 0,
            cl: 3,
        },
        ChangeRow {
            table_name: "papers".to_string(),
            pk: id.clone(),
            col_name: "title".to_string(),
            col_val: serde_json::Value::String("Resurrected Paper".to_string()),
            col_ver: 3,
            db_ver: 999,
            site_id: db_b.crr().site_id().await.unwrap(),
            seq: 1,
            cl: 3,
        },
    ];

    // Apply resurrection changeset to A
    let result = db_a.apply_changes(&resurrect_changes).await.unwrap();
    assert!(result.applied > 0, "Resurrection should be applied");

    // Paper should exist again on A with the new title
    let papers_a = db_a.list_papers().await.unwrap();
    assert_eq!(papers_a.len(), 1, "Paper should be resurrected");
    assert_eq!(papers_a[0].title, "Resurrected Paper");
}

#[tokio::test]
async fn test_column_before_sentinel_out_of_order() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let fake_id = uuid::Uuid::now_v7().to_string();
    let fake_site = vec![1u8; 16];

    // Send column changes BEFORE the sentinel (out-of-order delivery)
    let changes = vec![
        // Column change arrives first — row doesn't exist yet
        ChangeRow {
            table_name: "papers".to_string(),
            pk: fake_id.clone(),
            col_name: "title".to_string(),
            col_val: serde_json::Value::String("Out of Order Paper".to_string()),
            col_ver: 1,
            db_ver: 10,
            site_id: fake_site.clone(),
            seq: 1,
            cl: 1,
        },
        ChangeRow {
            table_name: "papers".to_string(),
            pk: fake_id.clone(),
            col_name: "is_favorite".to_string(),
            col_val: serde_json::Value::Number(1.into()),
            col_ver: 1,
            db_ver: 10,
            site_id: fake_site.clone(),
            seq: 2,
            cl: 1,
        },
        // Sentinel arrives after columns
        ChangeRow {
            table_name: "papers".to_string(),
            pk: fake_id.clone(),
            col_name: "__sentinel".to_string(),
            col_val: serde_json::Value::Null,
            col_ver: 1,
            db_ver: 10,
            site_id: fake_site.clone(),
            seq: 0,
            cl: 1,
        },
    ];

    let result = db.apply_changes(&changes).await.unwrap();
    assert!(result.applied > 0);

    // Paper should exist with correct values despite out-of-order delivery
    let papers = db.list_papers().await.unwrap();
    assert_eq!(
        papers.len(),
        1,
        "Paper should be created from out-of-order columns"
    );
    assert_eq!(papers[0].title, "Out of Order Paper");
    assert!(papers[0].status.is_favorite);
}

#[tokio::test]
async fn test_delete_resurrect_delete_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let id = db.insert_paper(&new_paper("Cycle Paper")).await.unwrap();
    let site = db.crr().site_id().await.unwrap();

    // Verify alive (CL=1)
    assert_eq!(db.list_papers().await.unwrap().len(), 1);

    // Delete (CL=2)
    db.delete_paper(&id).await.unwrap();
    assert_eq!(db.list_papers().await.unwrap().len(), 0);

    // Resurrect via changeset (CL=3)
    let resurrect = vec![ChangeRow {
        table_name: "papers".to_string(),
        pk: id.clone(),
        col_name: "__sentinel".to_string(),
        col_val: serde_json::Value::Null,
        col_ver: 3,
        db_ver: 9999,
        site_id: site.clone(),
        seq: 0,
        cl: 3,
    }];
    db.apply_changes(&resurrect).await.unwrap();
    assert_eq!(
        db.list_papers().await.unwrap().len(),
        1,
        "Should be resurrected at CL=3"
    );

    // Delete again (CL=4)
    let delete_again = vec![ChangeRow {
        table_name: "papers".to_string(),
        pk: id.clone(),
        col_name: "__sentinel".to_string(),
        col_val: serde_json::Value::Null,
        col_ver: 4,
        db_ver: 10000,
        site_id: site.clone(),
        seq: 0,
        cl: 4,
    }];
    db.apply_changes(&delete_again).await.unwrap();
    assert_eq!(
        db.list_papers().await.unwrap().len(),
        0,
        "Should be deleted again at CL=4"
    );
}

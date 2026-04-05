//! Robustness tests for CRR sync — edge cases, concurrent mutations,
//! idempotency, out-of-order application, delete/resurrect, junction tables.

use rotero_db::{annotations, collections, crr, notes, papers, saved_searches, schema, tags};
use rotero_models::{Annotation, AnnotationType, Collection, Note, Paper, Tag};

async fn open_test_db(dir: &std::path::Path) -> rotero_db::turso::Connection {
    std::fs::create_dir_all(dir).unwrap();
    let db_path = dir.join("test.db");
    let db = rotero_db::turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    schema::initialize_db(&conn).await.unwrap();
    conn
}

fn new_paper(title: &str) -> Paper {
    Paper::new(title.to_string())
}

/// Helper: set up two DBs with the same paper (same UUID), both with clocks.
async fn setup_two_devices_same_paper(
    dir_a: &std::path::Path,
    dir_b: &std::path::Path,
) -> (
    rotero_db::turso::Connection,
    rotero_db::turso::Connection,
    String,
) {
    let conn_a = open_test_db(dir_a).await;
    let conn_b = open_test_db(dir_b).await;

    let id = papers::insert_paper(&conn_a, &new_paper("Shared Paper"))
        .await
        .unwrap();

    // Replicate to B via sync
    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes).await.unwrap();

    // Verify B has it
    let papers_b = papers::list_papers(&conn_b).await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].id.as_deref(), Some(id.as_str()));

    (conn_a, conn_b, id)
}

// ── Delete vs Edit conflict ─────────────────────────────────────

#[tokio::test]
async fn test_delete_on_a_edit_on_b_delete_wins() {
    // Delete should win over edit because CL increases (delete CL=2 > edit CL=1)
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (conn_a, conn_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A deletes the paper
    papers::delete_paper(&conn_a, &id).await.unwrap();

    // B edits the paper (doesn't know about the delete yet)
    papers::set_favorite(&conn_b, &id, true).await.unwrap();

    // Sync A's changes to B
    let changes_a = crr::changes_since(&conn_a, 0).await.unwrap();
    let result = crr::apply_changes(&conn_b, &changes_a).await.unwrap();
    assert!(result.applied > 0);

    // Paper should be deleted on B (delete CL=2 beats alive CL=1)
    let papers_b = papers::list_papers(&conn_b).await.unwrap();
    assert_eq!(papers_b.len(), 0, "Delete should win over edit");
}

#[tokio::test]
async fn test_edit_on_a_delete_on_b_delete_wins() {
    // Same but reversed — B deletes, A edits
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (conn_a, conn_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A edits
    papers::set_read(&conn_a, &id, true).await.unwrap();

    // B deletes
    papers::delete_paper(&conn_b, &id).await.unwrap();

    // Sync B's changes to A
    let changes_b = crr::changes_since(&conn_b, 0).await.unwrap();
    crr::apply_changes(&conn_a, &changes_b).await.unwrap();

    let papers_a = papers::list_papers(&conn_a).await.unwrap();
    assert_eq!(papers_a.len(), 0, "Delete should win over edit");
}

// ── Idempotency ─────────────────────────────────────────────────

#[tokio::test]
async fn test_apply_same_changeset_twice_is_idempotent() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    // A inserts and updates
    let id = papers::insert_paper(&conn_a, &new_paper("Idempotent Paper"))
        .await
        .unwrap();
    papers::set_favorite(&conn_a, &id, true).await.unwrap();

    let changes = crr::changes_since(&conn_a, 0).await.unwrap();

    // Apply to B twice
    let result1 = crr::apply_changes(&conn_b, &changes).await.unwrap();
    let result2 = crr::apply_changes(&conn_b, &changes).await.unwrap();

    // Second application should skip everything
    assert!(result1.applied > 0);
    assert_eq!(result2.applied, 0, "Second application should be all skips");
    assert!(result2.skipped > 0);

    // Data should be correct
    let papers_b = papers::list_papers(&conn_b).await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].title, "Idempotent Paper");
    assert!(papers_b[0].is_favorite);
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

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    // A: insert → update title → update favorite
    let id = papers::insert_paper(&conn_a, &new_paper("Step 1"))
        .await
        .unwrap();
    let v1 = crr::current_db_version(&conn_a).await.unwrap();

    let paper = new_paper("Step 2");
    papers::update_paper_metadata(&conn_a, &id, &paper)
        .await
        .unwrap();
    let v2 = crr::current_db_version(&conn_a).await.unwrap();

    papers::set_favorite(&conn_a, &id, true).await.unwrap();

    // Export as two sequential batches (as real sync would)
    let batch1 = crr::changes_since(&conn_a, 0)
        .await
        .unwrap()
        .into_iter()
        .filter(|c| c.db_ver <= v1)
        .collect::<Vec<_>>();
    let batch2 = crr::changes_since(&conn_a, v1).await.unwrap();

    // Apply in correct order
    crr::apply_changes(&conn_b, &batch1).await.unwrap();
    crr::apply_changes(&conn_b, &batch2).await.unwrap();

    let papers_b = papers::list_papers(&conn_b).await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].title, "Step 2");
    assert!(papers_b[0].is_favorite);
}

// ── Multiple columns edited independently ───────────────────────

#[tokio::test]
async fn test_different_columns_merge_independently() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (conn_a, conn_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A changes favorite
    papers::set_favorite(&conn_a, &id, true).await.unwrap();

    // B changes read status
    papers::set_read(&conn_b, &id, true).await.unwrap();

    // Sync both ways
    let changes_a = crr::changes_since(&conn_a, 0).await.unwrap();
    let changes_b = crr::changes_since(&conn_b, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes_a).await.unwrap();
    crr::apply_changes(&conn_a, &changes_b).await.unwrap();

    // Both should have favorite=true AND read=true
    let papers_a = papers::list_papers(&conn_a).await.unwrap();
    let papers_b = papers::list_papers(&conn_b).await.unwrap();

    assert!(papers_a[0].is_favorite, "A should have favorite from A");
    assert!(papers_a[0].is_read, "A should have read from B");
    assert!(papers_b[0].is_favorite, "B should have favorite from A");
    assert!(papers_b[0].is_read, "B should have read from B");
}

// ── Convergence: both devices end up identical ──────────────────

#[tokio::test]
async fn test_bidirectional_sync_converges() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (conn_a, conn_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A: favorite + update title
    papers::set_favorite(&conn_a, &id, true).await.unwrap();
    let mut paper_a = new_paper("Title A");
    papers::update_paper_metadata(&conn_a, &id, &paper_a)
        .await
        .unwrap();

    // B: read + different title
    papers::set_read(&conn_b, &id, true).await.unwrap();
    let mut paper_b = new_paper("Title B");
    papers::update_paper_metadata(&conn_b, &id, &paper_b)
        .await
        .unwrap();

    // Round 1: sync A→B, B→A
    let changes_a = crr::changes_since(&conn_a, 0).await.unwrap();
    let changes_b = crr::changes_since(&conn_b, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes_a).await.unwrap();
    crr::apply_changes(&conn_a, &changes_b).await.unwrap();

    // Both should converge to the same state
    let pa = papers::list_papers(&conn_a).await.unwrap();
    let pb = papers::list_papers(&conn_b).await.unwrap();

    assert_eq!(pa[0].title, pb[0].title, "Titles should converge");
    assert_eq!(
        pa[0].is_favorite, pb[0].is_favorite,
        "Favorites should converge"
    );
    assert_eq!(pa[0].is_read, pb[0].is_read, "Read status should converge");
}

// ── Junction tables ─────────────────────────────────────────────

#[tokio::test]
async fn test_junction_table_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    // A: create paper, collection, add paper to collection
    let paper_id = papers::insert_paper(&conn_a, &new_paper("Junction Test"))
        .await
        .unwrap();
    let coll = Collection::new("Test Collection".to_string());
    let coll_id = collections::insert_collection(&conn_a, &coll)
        .await
        .unwrap();
    collections::add_paper_to_collection(&conn_a, &paper_id, &coll_id)
        .await
        .unwrap();

    // Sync to B
    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes).await.unwrap();

    // Verify B has the paper in the collection
    let ids_b = collections::list_paper_ids_in_collection(&conn_b, &coll_id)
        .await
        .unwrap();
    assert_eq!(ids_b.len(), 1);
    assert_eq!(ids_b[0], paper_id);
}

#[tokio::test]
async fn test_tag_junction_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    // A: create paper, tag, add tag to paper
    let paper_id = papers::insert_paper(&conn_a, &new_paper("Tagged Paper"))
        .await
        .unwrap();
    let tag_id = tags::get_or_create_tag(&conn_a, "machine-learning", None)
        .await
        .unwrap();
    tags::add_tag_to_paper(&conn_a, &paper_id, &tag_id)
        .await
        .unwrap();

    // Sync to B
    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes).await.unwrap();

    // Verify B has the tag and paper-tag association
    let tags_b = tags::list_tags(&conn_b).await.unwrap();
    assert!(tags_b.iter().any(|t| t.name == "machine-learning"));

    let tag_papers = tags::list_paper_ids_by_tag(&conn_b, &tag_id).await.unwrap();
    assert_eq!(tag_papers.len(), 1);
    assert_eq!(tag_papers[0], paper_id);
}

// ── Annotations sync ────────────────────────────────────────────

#[tokio::test]
async fn test_annotation_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    // A: create paper + annotation
    let paper_id = papers::insert_paper(&conn_a, &new_paper("Annotated Paper"))
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
    let ann_id = annotations::insert_annotation(&conn_a, &ann).await.unwrap();

    // Sync to B
    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes).await.unwrap();

    // Verify B has the annotation
    let anns_b = annotations::list_annotations_for_paper(&conn_b, &paper_id)
        .await
        .unwrap();
    assert_eq!(anns_b.len(), 1);
    assert_eq!(anns_b[0].content.as_deref(), Some("Important finding"));
    assert_eq!(anns_b[0].color, "#ffff00");
}

// ── Notes sync ──────────────────────────────────────────────────

#[tokio::test]
async fn test_notes_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    let paper_id = papers::insert_paper(&conn_a, &new_paper("Paper with Notes"))
        .await
        .unwrap();
    let note = Note::new(paper_id.clone(), "My Note".to_string());
    let note_id = notes::insert_note(&conn_a, &note).await.unwrap();

    // Sync to B
    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes).await.unwrap();

    let notes_b = notes::list_notes_for_paper(&conn_b, &paper_id)
        .await
        .unwrap();
    assert_eq!(notes_b.len(), 1);
    assert_eq!(notes_b[0].title, "My Note");
}

// ── Bulk operations ─────────────────────────────────────────────

#[tokio::test]
async fn test_bulk_sync_100_papers() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    // A: insert 100 papers
    for i in 0..100 {
        papers::insert_paper(&conn_a, &new_paper(&format!("Paper {i}")))
            .await
            .unwrap();
    }

    // Sync to B
    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    assert!(
        changes.len() > 100,
        "Should have many changes for 100 papers"
    );

    let result = crr::apply_changes(&conn_b, &changes).await.unwrap();
    assert!(result.applied > 0);

    let papers_b = papers::list_papers(&conn_b).await.unwrap();
    assert_eq!(papers_b.len(), 100, "B should have all 100 papers");
}

// ── Three-device convergence ────────────────────────────────────

#[tokio::test]
async fn test_three_device_convergence() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let dir_c = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;
    let conn_c = open_test_db(dir_c.path()).await;

    // A creates a paper
    let id = papers::insert_paper(&conn_a, &new_paper("Three Way"))
        .await
        .unwrap();

    // Sync A→B and A→C
    let changes_a = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes_a).await.unwrap();
    crr::apply_changes(&conn_c, &changes_a).await.unwrap();

    // Each device makes a different change
    papers::set_favorite(&conn_a, &id, true).await.unwrap();
    papers::set_read(&conn_b, &id, true).await.unwrap();
    let mut paper_c = new_paper("Updated by C");
    papers::update_paper_metadata(&conn_c, &id, &paper_c)
        .await
        .unwrap();

    // Gather all changes
    let ca = crr::changes_since(&conn_a, 0).await.unwrap();
    let cb = crr::changes_since(&conn_b, 0).await.unwrap();
    let cc = crr::changes_since(&conn_c, 0).await.unwrap();

    // Apply all to all (full mesh sync)
    crr::apply_changes(&conn_a, &cb).await.unwrap();
    crr::apply_changes(&conn_a, &cc).await.unwrap();
    crr::apply_changes(&conn_b, &ca).await.unwrap();
    crr::apply_changes(&conn_b, &cc).await.unwrap();
    crr::apply_changes(&conn_c, &ca).await.unwrap();
    crr::apply_changes(&conn_c, &cb).await.unwrap();

    // All three should converge
    let pa = papers::list_papers(&conn_a).await.unwrap();
    let pb = papers::list_papers(&conn_b).await.unwrap();
    let pc = papers::list_papers(&conn_c).await.unwrap();

    assert_eq!(pa[0].title, pb[0].title);
    assert_eq!(pb[0].title, pc[0].title);
    assert_eq!(pa[0].is_favorite, pb[0].is_favorite);
    assert_eq!(pa[0].is_read, pb[0].is_read);
    assert_eq!(pb[0].is_favorite, pc[0].is_favorite);
    assert_eq!(pb[0].is_read, pc[0].is_read);

    // All should have favorite=true and read=true
    assert!(pa[0].is_favorite);
    assert!(pa[0].is_read);
}

// ── Saved search sync ───────────────────────────────────────────

#[tokio::test]
async fn test_saved_search_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let conn_a = open_test_db(dir_a.path()).await;
    let conn_b = open_test_db(dir_b.path()).await;

    let search =
        rotero_models::SavedSearch::new("ML papers".to_string(), "machine learning".to_string());
    saved_searches::insert_saved_search(&conn_a, &search)
        .await
        .unwrap();

    let changes = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes).await.unwrap();

    let searches_b = saved_searches::list_saved_searches(&conn_b).await.unwrap();
    assert_eq!(searches_b.len(), 1);
    assert_eq!(searches_b[0].name, "ML papers");
    assert_eq!(searches_b[0].query, "machine learning");
}

// ── Resurrection ────────────────────────────────────────────────

#[tokio::test]
async fn test_resurrect_after_delete() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (conn_a, conn_b, id) = setup_two_devices_same_paper(dir_a.path(), dir_b.path()).await;

    // A deletes the paper (CL=2)
    papers::delete_paper(&conn_a, &id).await.unwrap();

    // Sync A→B: B now has CL=2, paper deleted
    let changes_a = crr::changes_since(&conn_a, 0).await.unwrap();
    crr::apply_changes(&conn_b, &changes_a).await.unwrap();
    let papers_b = papers::list_papers(&conn_b).await.unwrap();
    assert_eq!(papers_b.len(), 0, "Paper should be deleted after sync");

    // B resurrects: construct a changeset with CL=3 (odd = alive)
    // This simulates B explicitly re-creating the row after seeing the delete.
    let resurrect_changes = vec![
        crr::ChangeRow {
            table_name: "papers".to_string(),
            pk: id.clone(),
            col_name: "__sentinel".to_string(),
            col_val: serde_json::Value::Null,
            col_ver: 3, // CL=3 (alive, after delete CL=2)
            db_ver: 999,
            site_id: crr::site_id(&conn_b).await.unwrap(),
            seq: 0,
            cl: 3,
        },
        crr::ChangeRow {
            table_name: "papers".to_string(),
            pk: id.clone(),
            col_name: "title".to_string(),
            col_val: serde_json::Value::String("Resurrected Paper".to_string()),
            col_ver: 3,
            db_ver: 999,
            site_id: crr::site_id(&conn_b).await.unwrap(),
            seq: 1,
            cl: 3,
        },
    ];

    // Apply resurrection changeset to A
    let result = crr::apply_changes(&conn_a, &resurrect_changes)
        .await
        .unwrap();
    assert!(result.applied > 0, "Resurrection should be applied");

    // Paper should exist again on A with the new title
    let papers_a = papers::list_papers(&conn_a).await.unwrap();
    assert_eq!(papers_a.len(), 1, "Paper should be resurrected");
    assert_eq!(papers_a[0].title, "Resurrected Paper");
}

#[tokio::test]
async fn test_column_before_sentinel_out_of_order() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_test_db(dir.path()).await;

    let fake_id = uuid::Uuid::now_v7().to_string();
    let fake_site = vec![1u8; 16];

    // Send column changes BEFORE the sentinel (out-of-order delivery)
    let changes = vec![
        // Column change arrives first — row doesn't exist yet
        crr::ChangeRow {
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
        crr::ChangeRow {
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
        crr::ChangeRow {
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

    let result = crr::apply_changes(&conn, &changes).await.unwrap();
    assert!(result.applied > 0);

    // Paper should exist with correct values despite out-of-order delivery
    let papers = papers::list_papers(&conn).await.unwrap();
    assert_eq!(
        papers.len(),
        1,
        "Paper should be created from out-of-order columns"
    );
    assert_eq!(papers[0].title, "Out of Order Paper");
    assert!(papers[0].is_favorite);
}

#[tokio::test]
async fn test_delete_resurrect_delete_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_test_db(dir.path()).await;

    let id = papers::insert_paper(&conn, &new_paper("Cycle Paper"))
        .await
        .unwrap();
    let site = crr::site_id(&conn).await.unwrap();

    // Verify alive (CL=1)
    assert_eq!(papers::list_papers(&conn).await.unwrap().len(), 1);

    // Delete (CL=2)
    papers::delete_paper(&conn, &id).await.unwrap();
    assert_eq!(papers::list_papers(&conn).await.unwrap().len(), 0);

    // Resurrect via changeset (CL=3)
    let resurrect = vec![crr::ChangeRow {
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
    crr::apply_changes(&conn, &resurrect).await.unwrap();
    assert_eq!(
        papers::list_papers(&conn).await.unwrap().len(),
        1,
        "Should be resurrected at CL=3"
    );

    // Delete again (CL=4)
    let delete_again = vec![crr::ChangeRow {
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
    crr::apply_changes(&conn, &delete_again).await.unwrap();
    assert_eq!(
        papers::list_papers(&conn).await.unwrap().len(),
        0,
        "Should be deleted again at CL=4"
    );
}

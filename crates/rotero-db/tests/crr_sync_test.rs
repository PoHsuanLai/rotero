//! Integration test: two-device sync via CRR changesets.
//!
//! Creates two separate databases (simulating two devices), makes changes
//! on each, exports/imports changesets, and verifies the merge.

use rotero_db::Database;
use rotero_models::Paper;

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

fn new_paper(title: &str) -> Paper {
    Paper::new(title.to_string())
}

#[tokio::test]
async fn test_insert_tracks_changes() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    // Insert a paper
    let id = db.insert_paper(&new_paper("Test Paper")).await.unwrap();
    assert!(!id.is_empty());

    // Check that clock entries were created
    let changes = db.crr().changes_since(0).await.unwrap();
    assert!(
        !changes.is_empty(),
        "Should have clock entries after insert"
    );

    // Should have a sentinel + column entries for papers
    let paper_changes: Vec<_> = changes
        .iter()
        .filter(|c| c.table_name == "papers")
        .collect();
    assert!(
        paper_changes.len() > 1,
        "Should have sentinel + column entries"
    );

    // Sentinel should have CL=1 (alive)
    let sentinel = paper_changes
        .iter()
        .find(|c| c.col_name == "__sentinel")
        .unwrap();
    assert_eq!(sentinel.cl, 1, "Sentinel CL should be 1 (alive)");
}

#[tokio::test]
async fn test_update_increments_version() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let id = db.insert_paper(&new_paper("Paper A")).await.unwrap();
    let v1 = db.crr().current_db_version().await.unwrap();

    // Update favorite
    db.set_favorite(&id, true).await.unwrap();
    let v2 = db.crr().current_db_version().await.unwrap();
    assert!(v2 > v1, "db_version should increase after update");

    // Check the is_favorite column has a higher version
    let changes = db.crr().changes_since(v1).await.unwrap();
    let fav_change = changes
        .iter()
        .find(|c| c.col_name == "is_favorite")
        .unwrap();
    assert_eq!(
        fav_change.col_ver, 2,
        "col_ver should be 2 after one update"
    );
}

#[tokio::test]
async fn test_delete_sets_even_cl() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let id = db.insert_paper(&new_paper("To Delete")).await.unwrap();
    db.delete_paper(&id).await.unwrap();

    let changes = db.crr().changes_since(0).await.unwrap();
    let sentinel = changes
        .iter()
        .find(|c| c.table_name == "papers" && c.col_name == "__sentinel")
        .unwrap();
    assert_eq!(sentinel.cl, 2, "CL should be 2 (even = deleted)");
}

#[tokio::test]
async fn test_two_device_sync() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let sync_dir = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // Device A: insert a paper
    let paper_id = db_a.insert_paper(&new_paper("Shared Paper")).await.unwrap();
    db_a.set_favorite(&paper_id, true).await.unwrap();

    // Device A: insert a collection
    let coll = rotero_models::Collection::new("My Collection".to_string());
    db_a.insert_collection(&coll).await.unwrap();

    // Export from A
    let site_a = db_a.crr().site_id().await.unwrap();
    let engine_a =
        rotero_db::sync_test_helpers::TestSyncEngine::new(sync_dir.path().to_path_buf(), site_a);
    let exported = engine_a.export_changes(&db_a).await;
    assert!(exported > 0, "Should export changes from device A");

    // Import into B
    let site_b = db_b.crr().site_id().await.unwrap();
    let engine_b =
        rotero_db::sync_test_helpers::TestSyncEngine::new(sync_dir.path().to_path_buf(), site_b);
    let imported = engine_b.import_changes(&db_b).await;
    assert!(imported > 0, "Should import changes into device B");

    // Verify B has the paper
    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 1);
    assert_eq!(papers_b[0].title, "Shared Paper");
    assert!(papers_b[0].status.is_favorite);

    // Verify B has the collection
    let colls_b = db_b.list_collections().await.unwrap();
    assert_eq!(colls_b.len(), 1);
    assert_eq!(colls_b[0].name, "My Collection");
}

#[tokio::test]
async fn test_conflict_resolution_lww() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let db_a = open_test_db(dir_a.path()).await;
    let db_b = open_test_db(dir_b.path()).await;

    // Both devices start with the same paper (simulate by inserting with same ID)
    let paper = new_paper("Original Title");
    let id = db_a.insert_paper(&paper).await.unwrap();

    // Manually create the same paper on B with the same ID
    db_b.conn().execute(
        "INSERT INTO papers (id, title, authors, date_added, date_modified, is_favorite, is_read) VALUES (?1, ?2, '[]', ?3, ?3, 0, 0)",
        rotero_db::turso::params::Params::Positional(vec![
            rotero_db::turso::Value::Text(id.clone()),
            rotero_db::turso::Value::Text("Original Title".to_string()),
            rotero_db::turso::Value::Text(chrono::Utc::now().to_rfc3339()),
        ]),
    ).await.unwrap();
    // Set up initial clock on B too
    let _ = db_b
        .crr()
        .track_insert(
            "papers",
            &id,
            &[
                "title",
                "authors",
                "year",
                "doi",
                "abstract_text",
                "journal",
                "volume",
                "issue",
                "pages",
                "publisher",
                "url",
                "pdf_path",
                "date_added",
                "date_modified",
                "is_favorite",
                "is_read",
                "extra_meta",
                "citation_count",
                "citation_key",
            ],
        )
        .await;

    // Device A: update title (col_ver goes to 2)
    let mut paper_a = paper.clone();
    paper_a.title = "Title from A".to_string();
    db_a.update_paper_metadata(&id, &paper_a).await.unwrap();

    // Device B: update title (col_ver also goes to 2)
    let mut paper_b = paper.clone();
    paper_b.title = "Title from B".to_string();
    db_b.update_paper_metadata(&id, &paper_b).await.unwrap();

    // Export A's changes
    let changes_a = db_a.crr().changes_since(0).await.unwrap();
    // Apply A's changes to B
    let _result = db_b.crr().apply_changes(&changes_a).await.unwrap();

    // One of them should win deterministically (value comparison tie-break)
    let papers_b = db_b.list_papers().await.unwrap();
    assert_eq!(papers_b.len(), 1);
    let final_title = &papers_b[0].title;
    // With equal col_ver, the higher value wins (lexicographic)
    // "Title from B" > "Title from A", so B should win
    assert_eq!(
        final_title, "Title from B",
        "Higher value should win in LWW tie-break"
    );
}

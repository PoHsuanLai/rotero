//! Citation-edge storage and one-time-task flag accessors.

use rotero_db::Database;
use rotero_models::Paper;

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

async fn insert(db: &Database, title: &str) -> String {
    let paper = Paper {
        title: title.to_string(),
        ..Default::default()
    };
    db.insert_paper(&paper).await.unwrap()
}

#[tokio::test]
async fn citation_edges_round_trip_directed() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let a = insert(&db, "A").await;
    let b = insert(&db, "B").await;

    db.insert_citation(&a, &b).await.unwrap();
    // Idempotent — inserting the same edge twice is fine.
    db.insert_citation(&a, &b).await.unwrap();
    // Self-citation is rejected (no-op).
    db.insert_citation(&a, &a).await.unwrap();

    let pairs = db.list_all_citations().await.unwrap();
    assert_eq!(pairs, vec![(a.clone(), b.clone())]);
    // Direction is preserved: b→a is a different edge and wasn't inserted.
    assert!(!pairs.contains(&(b, a)));
}

#[tokio::test]
async fn app_flag_set_and_get() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    assert_eq!(db.get_app_flag("citations_scanned").await.unwrap(), None);
    db.set_app_flag("citations_scanned", "1").await.unwrap();
    assert_eq!(
        db.get_app_flag("citations_scanned")
            .await
            .unwrap()
            .as_deref(),
        Some("1")
    );
    // Upsert overwrites.
    db.set_app_flag("citations_scanned", "2").await.unwrap();
    assert_eq!(
        db.get_app_flag("citations_scanned")
            .await
            .unwrap()
            .as_deref(),
        Some("2")
    );
}

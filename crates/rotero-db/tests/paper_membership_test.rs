//! Per-paper tag/collection membership listing and removal.

use rotero_db::Database;
use rotero_models::{Collection, Paper};

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

async fn insert_paper(db: &Database, title: &str) -> String {
    let paper = Paper {
        title: title.to_string(),
        ..Default::default()
    };
    db.insert_paper(&paper).await.unwrap()
}

#[tokio::test]
async fn list_tags_for_paper_reflects_add_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "P").await;
    let other = insert_paper(&db, "Q").await;

    let red = db.get_or_create_tag("red", None).await.unwrap();
    let blue = db.get_or_create_tag("blue", None).await.unwrap();

    db.add_tag_to_paper(&paper, &red).await.unwrap();
    db.add_tag_to_paper(&paper, &blue).await.unwrap();
    // A tag on a different paper must not leak into this paper's list.
    db.add_tag_to_paper(&other, &red).await.unwrap();

    let mut names: Vec<String> = db
        .list_tags_for_paper(&paper)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["blue", "red"]);

    db.remove_tag_from_paper(&paper, &red).await.unwrap();
    let after: Vec<String> = db
        .list_tags_for_paper(&paper)
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(after, vec!["blue"]);

    // Removing again is a harmless no-op.
    db.remove_tag_from_paper(&paper, &red).await.unwrap();
    assert_eq!(db.list_tags_for_paper(&paper).await.unwrap().len(), 1);

    // The other paper is untouched.
    assert_eq!(db.list_tags_for_paper(&other).await.unwrap().len(), 1);
}

#[tokio::test]
async fn list_collections_for_paper_reflects_add_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let paper = insert_paper(&db, "P").await;

    let inbox = db
        .insert_collection(&Collection::new("Inbox".to_string()))
        .await
        .unwrap();
    let thesis = db
        .insert_collection(&Collection::new("Thesis".to_string()))
        .await
        .unwrap();

    db.add_paper_to_collection(&paper, &inbox).await.unwrap();
    db.add_paper_to_collection(&paper, &thesis).await.unwrap();

    let names: Vec<String> = db
        .list_collections_for_paper(&paper)
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    // Ordered by name.
    assert_eq!(names, vec!["Inbox", "Thesis"]);

    db.remove_paper_from_collection(&paper, &inbox)
        .await
        .unwrap();
    let after: Vec<String> = db
        .list_collections_for_paper(&paper)
        .await
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(after, vec!["Thesis"]);
}

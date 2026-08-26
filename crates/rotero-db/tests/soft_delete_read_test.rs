//! Deleted rows must not come back through any read path.
//!
//! Reads go through `<table>_live` views so a forgotten `deleted = 0` names a
//! relation that does not exist. Full-text search is the exception that needs
//! its own handling: `idx_papers_fts` is built on `papers` and cannot be
//! filtered, so a tombstoned paper still matches and has to be dropped after the
//! match. That gap is invisible locally until someone searches, which is exactly
//! the "I deleted this and it came back" complaint this design has to avoid.

mod common;

use rotero_db::Database;

async fn insert_paper(db: &Database, title: &str) -> String {
    db.insert_paper(&rotero_models::Paper {
        title: title.into(),
        abstract_text: Some("quantum entanglement in superconductors".into()),
        ..Default::default()
    })
    .await
    .unwrap()
}

/// Soft-delete a row directly, standing in for the tombstone writes that arrive
/// in the next step.
async fn tombstone(db: &Database, table: &str, id: &str) {
    db.conn()
        .execute(
            &format!("UPDATE {table} SET deleted = 1 WHERE id = ?1"),
            [turso::Value::Text(id.to_string())],
        )
        .await
        .unwrap();
}

/// Full-text search must not return a tombstoned paper.
#[tokio::test]
async fn fts_search_excludes_deleted_papers() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let keep = insert_paper(&db, "Quantum computing survey").await;
    let doomed = insert_paper(&db, "Quantum error correction").await;

    let found = db.search_papers("quantum").await.unwrap();
    assert_eq!(found.len(), 2, "both papers must match before deletion");

    tombstone(&db, "papers", &doomed).await;

    let found = db.search_papers("quantum").await.unwrap();
    let ids: Vec<&str> = found
        .iter()
        .filter_map(|p| p.id.as_deref())
        .collect();
    assert_eq!(
        ids,
        vec![keep.as_str()],
        "a tombstoned paper must not come back through full-text search"
    );
}

/// The ordinary list and count paths must not see a tombstoned paper.
#[tokio::test]
async fn list_and_count_exclude_deleted_papers() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    insert_paper(&db, "Kept").await;
    let doomed = insert_paper(&db, "Doomed").await;
    tombstone(&db, "papers", &doomed).await;

    assert_eq!(db.list_papers().await.unwrap().len(), 1, "list");
    assert_eq!(db.count_papers().await.unwrap(), 1, "count");
    assert!(
        db.get_papers_by_ids(std::slice::from_ref(&doomed))
            .await
            .unwrap()
            .is_empty(),
        "fetching a tombstoned paper by id must return nothing"
    );
}

/// Tombstoned tags and collections must disappear from their listings.
#[tokio::test]
async fn tags_and_collections_exclude_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let tag = db.get_or_create_tag("doomed", None).await.unwrap();
    let collection = db
        .insert_collection(&rotero_models::Collection::new("Doomed".into()))
        .await
        .unwrap();

    assert_eq!(db.list_tags().await.unwrap().len(), 1);
    assert_eq!(db.list_collections().await.unwrap().len(), 1);

    tombstone(&db, "tags", &tag).await;
    tombstone(&db, "collections", &collection).await;

    assert!(db.list_tags().await.unwrap().is_empty(), "tags");
    assert!(
        db.list_collections().await.unwrap().is_empty(),
        "collections"
    );
}

/// A tombstoned membership must not show up as a paper's tag or collection.
#[tokio::test]
async fn tombstoned_memberships_are_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let paper = insert_paper(&db, "Tagged").await;
    let tag = db.get_or_create_tag("keep", None).await.unwrap();
    db.add_tag_to_paper(&paper, &tag).await.unwrap();
    assert_eq!(db.list_tags_for_paper(&paper).await.unwrap().len(), 1);

    db.conn()
        .execute(
            "UPDATE paper_tags SET deleted = 1 WHERE paper_id = ?1 AND tag_id = ?2",
            [
                turso::Value::Text(paper.clone()),
                turso::Value::Text(tag.clone()),
            ],
        )
        .await
        .unwrap();

    assert!(
        db.list_tags_for_paper(&paper).await.unwrap().is_empty(),
        "a tombstoned membership must not be reported as a live one"
    );
}

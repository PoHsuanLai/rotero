//! Merging duplicates and deleting papers must reach other devices.
//!
//! Both operations touch junction and child tables directly. Rows written or
//! removed by a bare SQL statement never enter change tracking, so the result
//! stays on the machine that performed it — and the assertions here go through a
//! second device for exactly that reason. Asserting locally passes either way,
//! which is how both of these went unnoticed.

mod common;

use rotero_db::Database;
use rotero_db::sync_test_helpers::TestSyncEngine;

/// Two libraries plus the shared folder between them, with the temp dirs kept
/// alive for the length of the test.
struct Pair {
    a: Database,
    b: Database,
    engine_a: TestSyncEngine,
    engine_b: TestSyncEngine,
    _dirs: (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir),
}

impl Pair {
    async fn new() -> Self {
        let shared = tempfile::tempdir().unwrap();
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();

        let a = Database::open(dir_a.path().to_path_buf()).await.unwrap();
        let b = Database::open(dir_b.path().to_path_buf()).await.unwrap();

        let engine_a = TestSyncEngine::new(shared.path().to_path_buf(), vec![1; 16]);
        let engine_b = TestSyncEngine::new(shared.path().to_path_buf(), vec![2; 16]);

        Self {
            a,
            b,
            engine_a,
            engine_b,
            _dirs: (shared, dir_a, dir_b),
        }
    }

    /// Push everything A has to B.
    async fn sync_a_to_b(&self) {
        self.engine_a.export_changes(&self.a).await;
        self.engine_b.import_changes(&self.b).await;
    }
}

async fn insert_collection(db: &Database, name: &str) -> String {
    db.insert_collection(&rotero_models::Collection::new(name.into()))
        .await
        .unwrap()
}

/// Count *live* rows in a junction table matching one column, reading the table
/// directly.
///
/// The `list_*_for_paper` queries inner-join the parent table, so once a tag or
/// collection row is gone its orphaned junction rows are invisible through them
/// — on a healthy device and a broken one alike. Counting the junction table
/// itself is what distinguishes "the membership was removed" from "the
/// membership is still there but unreachable".
///
/// `deleted = 0` because removal is a tombstone, not a row deletion: the row has
/// to survive to carry the deletion to the other device, but it must not count
/// as a live membership on either.
async fn junction_count(db: &Database, table: &str, column: &str, value: &str) -> i64 {
    let mut rows = db
        .conn()
        .query(
            &format!("SELECT COUNT(*) FROM {table} WHERE {column} = ?1 AND deleted = 0"),
            [turso::Value::Text(value.to_string())],
        )
        .await
        .unwrap();
    rows.next()
        .await
        .unwrap()
        .and_then(|r| r.get_value(0).ok())
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(-1)
}

async fn insert_paper(db: &Database, title: &str) -> String {
    db.insert_paper(&rotero_models::Paper {
        title: title.into(),
        ..Default::default()
    })
    .await
    .unwrap()
}

/// Merging a duplicate must carry its tags to the surviving paper everywhere,
/// not just where the merge happened.
#[tokio::test]
async fn a_merged_paper_keeps_its_tags_on_the_other_device() {
    let p = Pair::new().await;

    let keep = insert_paper(&p.a, "Keeper").await;
    let dupe = insert_paper(&p.a, "Duplicate").await;
    let tag = p.a.get_or_create_tag("important", None).await.unwrap();
    let collection = insert_collection(&p.a, "Reading").await;

    p.a.add_tag_to_paper(&dupe, &tag).await.unwrap();
    p.a.add_paper_to_collection(&dupe, &collection)
        .await
        .unwrap();
    p.sync_a_to_b().await;

    p.a.merge_papers(&keep, &dupe).await.unwrap();
    p.sync_a_to_b().await;

    assert_eq!(
        p.b.list_tags_for_paper(&keep).await.unwrap().len(),
        1,
        "the surviving paper must carry the tag on the second device"
    );
    assert!(
        p.b.list_tags_for_paper(&dupe).await.unwrap().is_empty(),
        "the merged-away paper must not keep memberships pointing at it"
    );
    assert_eq!(
        p.b.list_papers().await.unwrap().len(),
        1,
        "the duplicate must be gone on the second device too"
    );
}

/// A membership the survivor already had must not be re-tracked by a merge.
///
/// `INSERT OR IGNORE` silently does nothing for a row that already exists, so
/// tracking it anyway would rewind a version a peer had already moved past.
#[tokio::test]
async fn merging_does_not_disturb_a_membership_both_papers_shared() {
    let p = Pair::new().await;

    let keep = insert_paper(&p.a, "Keeper").await;
    let dupe = insert_paper(&p.a, "Duplicate").await;
    let tag = p.a.get_or_create_tag("shared", None).await.unwrap();

    p.a.add_tag_to_paper(&keep, &tag).await.unwrap();
    p.a.add_tag_to_paper(&dupe, &tag).await.unwrap();

    let before = common::col_ver(&p.a, "paper_tags", &format!("{keep}:{tag}"), "__sentinel").await;
    p.a.merge_papers(&keep, &dupe).await.unwrap();
    let after = common::col_ver(&p.a, "paper_tags", &format!("{keep}:{tag}"), "__sentinel").await;

    assert_eq!(
        after, before,
        "an existing membership must not be re-tracked by the merge"
    );
    assert_eq!(p.a.list_tags_for_paper(&keep).await.unwrap().len(), 1);
}

/// Deleting a paper must remove its children on every device.
///
/// The schema declares `ON DELETE CASCADE`, but foreign keys are off, so nothing
/// fired and the rows leaked. Doing the deletes here also means each one is
/// tracked — a SQLite-level cascade would remove them silently and leave peers
/// holding memberships for a paper that no longer exists.
#[tokio::test]
async fn deleting_a_paper_removes_its_children_on_both_devices() {
    let p = Pair::new().await;

    let paper = insert_paper(&p.a, "Doomed").await;
    let tag = p.a.get_or_create_tag("temp", None).await.unwrap();
    let collection = insert_collection(&p.a, "Shelf").await;
    p.a.add_tag_to_paper(&paper, &tag).await.unwrap();
    p.a.add_paper_to_collection(&paper, &collection)
        .await
        .unwrap();

    p.sync_a_to_b().await;
    assert_eq!(p.b.list_tags_for_paper(&paper).await.unwrap().len(), 1);

    p.a.delete_paper(&paper).await.unwrap();

    assert!(
        p.a.list_tags_for_paper(&paper).await.unwrap().is_empty(),
        "children must be gone locally"
    );

    p.sync_a_to_b().await;
    assert!(
        p.b.list_tags_for_paper(&paper).await.unwrap().is_empty(),
        "the second device must drop the memberships too, not keep orphans"
    );
    assert!(p.b.list_papers().await.unwrap().is_empty());
}

/// Deleting a collection must remove its memberships on every device.
///
/// The doc comment claimed a cascade the code never performed: foreign keys are
/// off, so `DELETE FROM collections` left every `paper_collections` row behind,
/// untracked. Locally the collection vanished and the orphans were invisible,
/// which is why this went unnoticed — the second device is what exposes it,
/// still holding a membership pointing at a collection that no longer exists.
#[tokio::test]
async fn deleting_a_collection_removes_its_memberships_on_both_devices() {
    let p = Pair::new().await;

    let paper = insert_paper(&p.a, "Shelved").await;
    let collection = insert_collection(&p.a, "Doomed shelf").await;
    p.a.add_paper_to_collection(&paper, &collection)
        .await
        .unwrap();

    p.sync_a_to_b().await;
    assert_eq!(
        junction_count(&p.b, "paper_collections", "collection_id", &collection).await,
        1,
        "the membership must reach the second device first"
    );

    p.a.delete_collection(&collection).await.unwrap();

    assert_eq!(
        junction_count(&p.a, "paper_collections", "collection_id", &collection).await,
        0,
        "the membership must no longer be live locally, not merely unreachable"
    );

    p.sync_a_to_b().await;
    assert_eq!(
        junction_count(&p.b, "paper_collections", "collection_id", &collection).await,
        0,
        "the second device must drop the membership too, not keep an orphan"
    );
    assert!(
        p.b.list_papers().await.unwrap().len() == 1,
        "the paper itself must survive its collection being deleted"
    );
}

/// Deleting a tag must remove its associations on every device.
///
/// Same defect as [`deleting_a_collection_removes_its_memberships_on_both_devices`]:
/// `DELETE FROM tags` left every `paper_tags` row behind and untracked, so peers
/// kept associations pointing at a tag that no longer exists.
#[tokio::test]
async fn deleting_a_tag_removes_its_associations_on_both_devices() {
    let p = Pair::new().await;

    let paper = insert_paper(&p.a, "Tagged").await;
    let tag = p.a.get_or_create_tag("doomed", None).await.unwrap();
    p.a.add_tag_to_paper(&paper, &tag).await.unwrap();

    p.sync_a_to_b().await;
    assert_eq!(
        junction_count(&p.b, "paper_tags", "tag_id", &tag).await,
        1,
        "the association must reach the second device first"
    );

    p.a.delete_tag(&tag).await.unwrap();

    assert_eq!(
        junction_count(&p.a, "paper_tags", "tag_id", &tag).await,
        0,
        "the association must no longer be live locally, not merely unreachable"
    );

    p.sync_a_to_b().await;
    assert_eq!(
        junction_count(&p.b, "paper_tags", "tag_id", &tag).await,
        0,
        "the second device must drop the association too, not keep an orphan"
    );
    assert!(
        p.b.list_papers().await.unwrap().len() == 1,
        "the paper itself must survive its tag being deleted"
    );
}

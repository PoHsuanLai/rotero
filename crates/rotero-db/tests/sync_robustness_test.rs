//! Multi-device scenarios the sync engine has to survive.
//!
//! Ported from the changeset-era suite. The mechanism changed — whole-row
//! snapshots rather than per-column change rows — but the situations did not: a
//! delete racing an edit, the same data arriving twice, three devices
//! exchanging in whatever order they happen to.
//!
//! Every assertion goes through a second device. A local read succeeds whether
//! or not the change ever left the machine, which is how the original bugs went
//! unnoticed in the first place.

mod common;

use rotero_db::Database;
use rotero_db::sync_test_helpers::TestSyncEngine;

/// Two devices sharing a folder, each with its own library.
struct Devices {
    a: Database,
    b: Database,
    engine_a: TestSyncEngine,
    engine_b: TestSyncEngine,
    _shared: tempfile::TempDir,
    _dir_a: tempfile::TempDir,
    _dir_b: tempfile::TempDir,
}

impl Devices {
    async fn new() -> Self {
        let shared = tempfile::tempdir().unwrap();
        let dir_a = tempfile::tempdir().unwrap();
        let dir_b = tempfile::tempdir().unwrap();
        let a = common::open_test_db(dir_a.path()).await;
        let b = common::open_test_db(dir_b.path()).await;
        let engine_a = TestSyncEngine::new(shared.path().to_path_buf(), vec![0xaa; 16]);
        let engine_b = TestSyncEngine::new(shared.path().to_path_buf(), vec![0xbb; 16]);
        Self {
            a,
            b,
            engine_a,
            engine_b,
            _shared: shared,
            _dir_a: dir_a,
            _dir_b: dir_b,
        }
    }

    async fn a_to_b(&self) {
        self.engine_a.export_changes(&self.a).await;
        self.engine_b.import_changes(&self.b).await;
    }

    async fn b_to_a(&self) {
        self.engine_b.export_changes(&self.b).await;
        self.engine_a.import_changes(&self.a).await;
    }

    /// Exchange until both devices have seen everything.
    async fn converge(&self) {
        for _ in 0..3 {
            self.a_to_b().await;
            self.b_to_a().await;
        }
    }
}

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

async fn retitle(db: &Database, id: &str, title: &str) {
    db.update_paper_metadata(
        id,
        &rotero_models::Paper {
            title: title.into(),
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

/// A delete made after an edit wins on both devices.
#[tokio::test]
async fn a_later_delete_beats_an_earlier_edit() {
    let d = Devices::new().await;
    let paper = insert_paper(&d.a, "Original").await;
    d.a_to_b().await;

    retitle(&d.b, &paper, "Edited on B").await;
    d.b_to_a().await;
    d.a.delete_paper(&paper).await.unwrap();

    d.converge().await;
    assert!(titles(&d.a).await.is_empty(), "device A");
    assert!(
        titles(&d.b).await.is_empty(),
        "the delete must reach B, not just A"
    );
}

/// An edit made after a delete resurrects the row on both devices.
#[tokio::test]
async fn a_later_edit_beats_an_earlier_delete() {
    let d = Devices::new().await;
    let paper = insert_paper(&d.a, "Original").await;
    d.a_to_b().await;

    d.a.delete_paper(&paper).await.unwrap();
    d.a_to_b().await;
    assert!(titles(&d.b).await.is_empty(), "B must see the delete first");

    // B brings it back by re-creating the row with the same id.
    d.b.conn()
        .execute(
            "UPDATE papers SET deleted = 0, title = 'Resurrected', \
             updated_at = ?2, updated_by = 'bb' WHERE id = ?1",
            turso::params::Params::Positional(vec![
                turso::Value::Text(paper.clone()),
                turso::Value::Integer(chrono::Utc::now().timestamp_millis() + 1_000),
            ]),
        )
        .await
        .unwrap();

    d.converge().await;
    assert_eq!(titles(&d.a).await, vec!["Resurrected"], "device A");
    assert_eq!(titles(&d.b).await, vec!["Resurrected"], "device B");
}

/// Applying the same snapshot twice changes nothing the second time.
#[tokio::test]
async fn importing_twice_is_idempotent() {
    let d = Devices::new().await;
    insert_paper(&d.a, "Once").await;

    d.engine_a.export_changes(&d.a).await;
    let first = d.engine_b.import_changes(&d.b).await;
    let second = d.engine_b.import_changes(&d.b).await;

    assert!(first > 0, "the first import must apply something");
    assert_eq!(second, 0, "the second must apply nothing");
    assert_eq!(titles(&d.b).await, vec!["Once"]);
}

/// Edits made on both devices while apart both survive.
#[tokio::test]
async fn bidirectional_edits_converge() {
    let d = Devices::new().await;
    insert_paper(&d.a, "From A").await;
    insert_paper(&d.b, "From B").await;

    d.converge().await;

    let expected = vec!["From A".to_string(), "From B".to_string()];
    assert_eq!(titles(&d.a).await, expected, "device A");
    assert_eq!(titles(&d.b).await, expected, "device B");
}

/// Tags, collections, and their memberships reach the other device.
#[tokio::test]
async fn junctions_sync() {
    let d = Devices::new().await;
    let paper = insert_paper(&d.a, "Tagged and shelved").await;
    let tag = d.a.get_or_create_tag("method", None).await.unwrap();
    let collection =
        d.a.insert_collection(&rotero_models::Collection::new("Shelf".into()))
            .await
            .unwrap();
    d.a.add_tag_to_paper(&paper, &tag).await.unwrap();
    d.a.add_paper_to_collection(&paper, &collection)
        .await
        .unwrap();

    d.converge().await;

    assert_eq!(
        d.b.list_tags_for_paper(&paper).await.unwrap().len(),
        1,
        "tag"
    );
    assert_eq!(
        d.b.list_collections_for_paper(&paper).await.unwrap().len(),
        1,
        "collection"
    );
}

/// Annotations and notes reach the other device.
#[tokio::test]
async fn annotations_and_notes_sync() {
    let d = Devices::new().await;
    let paper = insert_paper(&d.a, "Annotated").await;

    d.a.insert_note(&rotero_models::Note::new(paper.clone(), "A thought".into()))
        .await
        .unwrap();

    d.converge().await;

    let notes = d.b.list_notes_for_paper(&paper).await.unwrap();
    assert_eq!(notes.len(), 1, "the note must reach the second device");
    assert_eq!(notes[0].title, "A thought");
}

/// A larger library still converges.
#[tokio::test]
async fn a_hundred_papers_sync() {
    let d = Devices::new().await;
    for i in 0..100 {
        insert_paper(&d.a, &format!("Paper {i:03}")).await;
    }

    d.converge().await;

    assert_eq!(
        titles(&d.b).await.len(),
        100,
        "every paper must reach the second device"
    );
}

/// Three devices reach the same state.
#[tokio::test]
async fn three_devices_converge() {
    let shared = tempfile::tempdir().unwrap();
    let dirs: Vec<_> = (0..3).map(|_| tempfile::tempdir().unwrap()).collect();
    let mut dbs = Vec::new();
    let mut engines = Vec::new();
    for (i, dir) in dirs.iter().enumerate() {
        dbs.push(common::open_test_db(dir.path()).await);
        engines.push(TestSyncEngine::new(
            shared.path().to_path_buf(),
            vec![i as u8 + 1; 16],
        ));
    }

    for (i, db) in dbs.iter().enumerate() {
        insert_paper(db, &format!("From {i}")).await;
    }

    for _ in 0..3 {
        for (engine, db) in engines.iter().zip(&dbs) {
            engine.export_changes(db).await;
        }
        for (engine, db) in engines.iter().zip(&dbs) {
            engine.import_changes(db).await;
        }
    }

    let expected = vec![
        "From 0".to_string(),
        "From 1".to_string(),
        "From 2".to_string(),
    ];
    for (i, db) in dbs.iter().enumerate() {
        assert_eq!(titles(db).await, expected, "device {i} diverged");
    }
}

/// A saved search reaches the other device.
#[tokio::test]
async fn saved_searches_sync() {
    let d = Devices::new().await;
    d.a.insert_saved_search(&rotero_models::SavedSearch::new(
        "ML papers".into(),
        "machine learning".into(),
    ))
    .await
    .unwrap();

    d.converge().await;

    let searches = d.b.list_saved_searches().await.unwrap();
    assert_eq!(searches.len(), 1);
    assert_eq!(searches[0].name, "ML papers");
}

/// Deleting the same row on both devices independently is not a conflict.
#[tokio::test]
async fn concurrent_deletes_agree() {
    let d = Devices::new().await;
    let paper = insert_paper(&d.a, "Doomed twice").await;
    d.converge().await;

    d.a.delete_paper(&paper).await.unwrap();
    d.b.delete_paper(&paper).await.unwrap();

    d.converge().await;

    assert!(titles(&d.a).await.is_empty(), "device A");
    assert!(titles(&d.b).await.is_empty(), "device B");
}

/// Losing the shared folder's state does not lose data.
///
/// The changeset engine tracked a per-peer cursor, and a bug in sharing it
/// silently dropped changes. Snapshots carry whole tables, so there is no cursor
/// to corrupt — this asserts that property rather than assuming it.
#[tokio::test]
async fn sync_survives_losing_local_state() {
    let d = Devices::new().await;
    insert_paper(&d.a, "Before").await;
    d.converge().await;

    // Wipe everything in the shared folder except the device snapshots.
    for entry in std::fs::read_dir(d._shared.path()).unwrap().flatten() {
        if entry.path().is_file() {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }

    insert_paper(&d.a, "After").await;
    d.converge().await;

    assert_eq!(
        titles(&d.b).await,
        vec!["After".to_string(), "Before".to_string()],
        "both papers must survive losing the folder's bookkeeping"
    );
}

/// Removing a tag from a paper reaches the other device, and stays removed.
///
/// Found by the generated schedules in `sync_props`. The removal used to delete
/// the junction row and then tombstone it, but the delete left nothing for the
/// tombstone to stamp: the removal never reached B, and B's surviving copy then
/// put it back on A — undoing, on the device that made it, what the user had
/// just done. Asserting on A alone passes either way, which is how it hid.
#[tokio::test]
async fn removing_a_tag_reaches_the_other_device() {
    let d = Devices::new().await;

    let paper = insert_paper(&d.a, "Tagged").await;
    let tag = d.a.get_or_create_tag("ml", None).await.unwrap();
    d.a.add_tag_to_paper(&paper, &tag).await.unwrap();
    d.converge().await;
    assert_eq!(
        d.b.list_tags_for_paper(&paper).await.unwrap().len(),
        1,
        "B must first have the membership for its removal to mean anything"
    );

    d.a.remove_tag_from_paper(&paper, &tag).await.unwrap();
    d.converge().await;

    assert!(
        d.b.list_tags_for_paper(&paper).await.unwrap().is_empty(),
        "the removal must reach B"
    );
    assert!(
        d.a.list_tags_for_paper(&paper).await.unwrap().is_empty(),
        "and must not come back on A when B's copy is merged again"
    );
}

/// The same, for a paper's collection membership.
#[tokio::test]
async fn removing_a_paper_from_a_collection_reaches_the_other_device() {
    let d = Devices::new().await;

    let paper = insert_paper(&d.a, "Filed").await;
    let collection =
        d.a.insert_collection(&rotero_models::Collection::new("Reading".into()))
            .await
            .unwrap();
    d.a.add_paper_to_collection(&paper, &collection)
        .await
        .unwrap();
    d.converge().await;
    assert_eq!(
        d.b.list_paper_ids_in_collection(&collection)
            .await
            .unwrap()
            .len(),
        1,
        "B must first have the membership"
    );

    d.a.remove_paper_from_collection(&paper, &collection)
        .await
        .unwrap();
    d.converge().await;

    assert!(
        d.b.list_paper_ids_in_collection(&collection)
            .await
            .unwrap()
            .is_empty(),
        "the removal must reach B"
    );
    assert!(
        d.a.list_paper_ids_in_collection(&collection)
            .await
            .unwrap()
            .is_empty(),
        "and must not come back on A"
    );
}

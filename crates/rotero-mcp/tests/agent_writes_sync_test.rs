//! Writes made through the MCP server must reach the user's other devices.
//!
//! The agent writes through its own `Database` wrapper, which shares the app's
//! connection and CRR store but has its own copy of each write method. Several
//! of those executed their statement and called `notify()` without tracking the
//! change, so a note the agent took saved locally, appeared in the UI, and never
//! left the machine.
//!
//! Every assertion here goes through a second device, since the local read
//! succeeds either way.

use rotero_db::sync_test_helpers::TestSyncEngine;

/// Open an app database and an MCP handle sharing its connection and CRR store.
async fn app_and_agent(dir: &std::path::Path) -> (rotero_db::Database, rotero_mcp::Database) {
    let app = rotero_db::Database::open(dir.to_path_buf()).await.unwrap();
    let agent = rotero_mcp::Database::from_db(&app);
    (app, agent)
}

async fn insert_paper(db: &rotero_db::Database, title: &str) -> String {
    db.insert_paper(&rotero_models::Paper {
        title: title.into(),
        ..Default::default()
    })
    .await
    .unwrap()
}

/// A note the agent writes, and a later edit to it, must both reach a peer.
#[tokio::test]
async fn agent_notes_reach_a_second_device() {
    let shared = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (app_a, agent_a) = app_and_agent(dir_a.path()).await;
    let app_b = rotero_db::Database::open(dir_b.path().to_path_buf())
        .await
        .unwrap();

    let paper = insert_paper(&app_a, "Paper").await;
    let note = agent_a
        .insert_note(&paper, "Summary", "The agent's first pass")
        .await
        .unwrap();

    let engine_a = TestSyncEngine::new(shared.path().to_path_buf(), vec![1; 16]);
    let engine_b = TestSyncEngine::new(shared.path().to_path_buf(), vec![2; 16]);
    engine_a.export_changes(&app_a).await;
    engine_b.import_changes(&app_b).await;

    let notes = app_b.list_notes_for_paper(&paper).await.unwrap();
    assert_eq!(
        notes.len(),
        1,
        "a note written by the agent must reach the second device"
    );
    assert_eq!(notes[0].body, "The agent's first pass");

    // And an edit to it must follow.
    agent_a
        .update_note(&note, "Summary", "Revised after reading")
        .await
        .unwrap();
    engine_a.export_changes(&app_a).await;
    engine_b.import_changes(&app_b).await;

    let notes = app_b.list_notes_for_paper(&paper).await.unwrap();
    assert_eq!(
        notes[0].body, "Revised after reading",
        "an edit by the agent must reach the second device too"
    );
}

/// Tags the agent creates and attaches must reach a peer.
#[tokio::test]
async fn agent_tags_reach_a_second_device() {
    let shared = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (app_a, agent_a) = app_and_agent(dir_a.path()).await;
    let app_b = rotero_db::Database::open(dir_b.path().to_path_buf())
        .await
        .unwrap();

    let paper = insert_paper(&app_a, "Paper").await;
    let tag = agent_a
        .get_or_create_tag("agent-picked", None)
        .await
        .unwrap();
    agent_a.add_tag_to_paper(&paper, &tag).await.unwrap();

    let engine_a = TestSyncEngine::new(shared.path().to_path_buf(), vec![1; 16]);
    let engine_b = TestSyncEngine::new(shared.path().to_path_buf(), vec![2; 16]);
    engine_a.export_changes(&app_a).await;
    engine_b.import_changes(&app_b).await;

    let tags = app_b.list_tags_for_paper(&paper).await.unwrap();
    assert_eq!(
        tags.len(),
        1,
        "a tag the agent attached must reach the second device"
    );
    assert_eq!(tags[0].name, "agent-picked");
}

/// The agent's delete must remove children everywhere, like the app's.
///
/// This one delegates to `rotero_db::Database::delete_paper` rather than
/// reimplementing it — the two had already drifted, and only the app's copy
/// removed the children.
#[tokio::test]
async fn an_agent_delete_removes_children_on_both_devices() {
    let shared = tempfile::tempdir().unwrap();
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (app_a, agent_a) = app_and_agent(dir_a.path()).await;
    let app_b = rotero_db::Database::open(dir_b.path().to_path_buf())
        .await
        .unwrap();

    let paper = insert_paper(&app_a, "Doomed").await;
    let tag = app_a.get_or_create_tag("temp", None).await.unwrap();
    app_a.add_tag_to_paper(&paper, &tag).await.unwrap();

    let engine_a = TestSyncEngine::new(shared.path().to_path_buf(), vec![1; 16]);
    let engine_b = TestSyncEngine::new(shared.path().to_path_buf(), vec![2; 16]);
    engine_a.export_changes(&app_a).await;
    engine_b.import_changes(&app_b).await;
    assert_eq!(app_b.list_tags_for_paper(&paper).await.unwrap().len(), 1);

    agent_a.delete_paper(&paper).await.unwrap();
    engine_a.export_changes(&app_a).await;
    engine_b.import_changes(&app_b).await;

    assert!(
        app_b.list_tags_for_paper(&paper).await.unwrap().is_empty(),
        "the second device must drop memberships for a paper the agent deleted"
    );
    assert!(app_b.list_papers().await.unwrap().is_empty());
}

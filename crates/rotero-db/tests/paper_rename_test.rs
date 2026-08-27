//! Covers `update_paper_title`, the write behind the UI's rename action.
//!
//! Renaming deliberately does not go through `update_paper_metadata`, which
//! rewrites every bibliographic column. These tests pin the property that
//! distinction exists for: unrelated fields keep their values, rather than being
//! blanked by a full-row rewrite from a stale `Paper`.

mod common;

use common::open_test_db;
use rotero_models::{Paper, Publication};

fn sample_paper() -> Paper {
    Paper {
        title: "Attention Is All You Need".into(),
        year: Some(2017),
        doi: Some("10.48550/arXiv.1706.03762".into()),
        abstract_text: Some("The dominant sequence transduction models…".into()),
        publication: Publication {
            journal: Some("NeurIPS".into()),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn rename_replaces_only_the_title() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = db.insert_paper(&sample_paper()).await.unwrap();

    db.update_paper_title(&id, "Attention Is All You Need (v2)")
        .await
        .unwrap();

    let papers = db.list_papers().await.unwrap();
    let p = papers
        .iter()
        .find(|p| p.id.as_deref() == Some(&id))
        .unwrap();
    assert_eq!(p.title, "Attention Is All You Need (v2)");
    // The whole point of a title-only write: a rename must not blank the fields
    // enrichment filled in, which a full-row rewrite from a stale Paper would.
    assert_eq!(p.year, Some(2017));
    // Stored in canonical arXiv form, as insert wrote it.
    assert_eq!(p.doi.as_deref(), Some("arXiv:1706.03762"));
    assert_eq!(p.publication.journal.as_deref(), Some("NeurIPS"));
    assert!(p.abstract_text.is_some());
}

/// Renaming has to advance the row's sync clock, or the new title never leaves
/// this device.
///
/// This test previously asserted the stronger property that renaming advanced
/// the *title's* clock without touching the DOI's, so a rename could not
/// overwrite a peer's newer DOI. Sync is now last-writer-wins per row rather
/// than per column, so that isolation no longer exists: whichever device writes
/// later wins the whole row. The tradeoff is deliberate — per-column clocks are
/// most of the machinery the sync rewrite removed — and it costs something only
/// when two devices edit different fields of the same paper inside one sync
/// window. What still matters, and is asserted here, is that the rename is
/// stamped at all.
#[tokio::test]
async fn rename_advances_the_sync_clock() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = db.insert_paper(&sample_paper()).await.unwrap();

    let stamp = |id: String| {
        let db = db.clone();
        async move {
            let mut rows = db
                .conn()
                .query(
                    "SELECT updated_at FROM papers WHERE id = ?1",
                    [turso::Value::Text(id)],
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
    };

    let before = stamp(id.clone()).await;
    db.update_paper_title(&id, "Renamed").await.unwrap();
    let after = stamp(id.clone()).await;

    assert!(
        after > before,
        "renaming must advance the row's sync clock ({after} !> {before}), or \
         the new title stays on this device"
    );
}

/// The FTS index covers `title`, and the app's search box goes through
/// `search_papers`. A rename that updated the row but not the index would leave
/// the paper findable only under its old name.
#[tokio::test]
async fn rename_is_reflected_in_search() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = db.insert_paper(&sample_paper()).await.unwrap();

    db.update_paper_title(&id, "Sparse Autoencoders For Interpretability")
        .await
        .unwrap();

    let hits = db.search_papers("Sparse Autoencoders").await.unwrap();
    assert!(
        hits.iter().any(|p| p.id.as_deref() == Some(&id)),
        "the renamed paper must be findable under its new title"
    );

    let stale = db.search_papers("Attention Is All You Need").await.unwrap();
    assert!(
        !stale.iter().any(|p| p.id.as_deref() == Some(&id)),
        "the renamed paper must no longer match its old title"
    );
}

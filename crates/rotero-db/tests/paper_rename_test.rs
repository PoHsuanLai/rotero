//! Covers `update_paper_title`, the write behind the UI's rename action.
//!
//! Renaming deliberately does not go through `update_paper_metadata`, which
//! rewrites every bibliographic column. These tests pin the two properties that
//! distinction exists for: unrelated fields keep their values, and only the
//! title's sync clock advances.

mod common;

use common::{col_ver, open_test_db};
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

#[tokio::test]
async fn rename_bumps_only_the_title_clock() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = db.insert_paper(&sample_paper()).await.unwrap();

    let doi_before = col_ver(&db, "papers", &id, "doi").await;
    let title_before = col_ver(&db, "papers", &id, "title").await;

    db.update_paper_title(&id, "Renamed").await.unwrap();

    assert!(
        col_ver(&db, "papers", &id, "title").await > title_before,
        "renaming must advance the title's column version so the change syncs"
    );
    assert_eq!(
        col_ver(&db, "papers", &id, "doi").await,
        doi_before,
        "renaming must not mark untouched columns dirty — that would let a \
         rename overwrite a peer's newer DOI on merge"
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

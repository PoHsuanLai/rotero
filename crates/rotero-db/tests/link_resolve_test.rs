//! Resolving a PDF's external link URL back to a library paper via
//! `Database::find_paper_by_link` — DOI, arXiv, and raw-URL matching.

use rotero_db::Database;
use rotero_models::{Paper, PaperLinks};

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

/// Inserts a paper and returns its DB-assigned id.
async fn insert(db: &Database, title: &str, doi: Option<&str>, url: Option<&str>) -> String {
    let paper = Paper {
        title: title.to_string(),
        doi: doi.map(str::to_string),
        links: PaperLinks {
            url: url.map(str::to_string),
            ..Default::default()
        },
        ..Default::default()
    };
    db.insert_paper(&paper).await.unwrap()
}

#[tokio::test]
async fn resolves_doi_url_to_paper() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = insert(&db, "Nature paper", Some("10.1038/nature12373"), None).await;

    // A doi.org link URL should find the paper stored with the bare DOI.
    let hit = db
        .find_paper_by_link("https://doi.org/10.1038/nature12373")
        .await
        .unwrap();
    assert_eq!(hit.and_then(|p| p.id), Some(id));
}

#[tokio::test]
async fn resolves_arxiv_url_to_paper() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    // Stored in the canonical arXiv form.
    let id = insert(&db, "arXiv paper", Some("arXiv:2401.11660"), None).await;

    // Versioned arXiv URL still matches the unversioned stored id.
    let hit = db
        .find_paper_by_link("https://arxiv.org/abs/2401.11660v2")
        .await
        .unwrap();
    assert_eq!(hit.and_then(|p| p.id), Some(id));
}

#[tokio::test]
async fn resolves_arxiv_url_to_paper_stored_as_arxiv_doi() {
    // Regression: real libraries often store arXiv papers as their raw arXiv-DOI
    // (`10.48550/arXiv.X`) rather than the canonical `arXiv:X`. A `/abs/X` link
    // must still resolve. (Found against the live library — 9 such near-misses.)
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = insert(&db, "DIAYN", Some("10.48550/arXiv.1802.06070"), None).await;

    let hit = db
        .find_paper_by_link("https://arxiv.org/abs/1802.06070")
        .await
        .unwrap();
    assert_eq!(hit.and_then(|p| p.id), Some(id));
}

#[tokio::test]
async fn resolves_doi_case_insensitively() {
    // DOIs are case-insensitive by spec; a link's case may differ from storage.
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = insert(&db, "Mixed-case DOI", Some("10.1145/AbC.123"), None).await;

    let hit = db
        .find_paper_by_link("https://doi.org/10.1145/abc.123")
        .await
        .unwrap();
    assert_eq!(hit.and_then(|p| p.id), Some(id));
}

#[tokio::test]
async fn resolves_raw_url_to_paper() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    let id = insert(
        &db,
        "Web paper",
        None,
        Some("https://example.com/paper.html"),
    )
    .await;

    let hit = db
        .find_paper_by_link("https://example.com/paper.html")
        .await
        .unwrap();
    assert_eq!(hit.and_then(|p| p.id), Some(id));
}

#[tokio::test]
async fn no_match_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;
    insert(&db, "Nature paper", Some("10.1038/nature12373"), None).await;

    let hit = db
        .find_paper_by_link("https://doi.org/10.9999/nope")
        .await
        .unwrap();
    assert!(hit.is_none());
}

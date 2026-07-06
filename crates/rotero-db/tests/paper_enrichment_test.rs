//! Round-trips the enriched `Paper` fields (item_type, typed creators with
//! roles, and the `extra_meta`-backed venue fields) through insert → read, and
//! verifies backward compatibility with legacy rows whose `authors` column holds
//! a bare `Vec<String>`.

use rotero_db::Database;
use rotero_models::{Creator, CreatorRole, Paper};

async fn open_test_db(dir: &std::path::Path) -> Database {
    Database::open(dir.to_path_buf()).await.unwrap()
}

#[tokio::test]
async fn enriched_fields_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let mut paper = Paper::new("A Book About Things".to_string());
    paper.item_type = "book".to_string();
    paper.creators = vec![
        Creator::author("Ada", "Lovelace"),
        Creator {
            first_name: "Ed".into(),
            last_name: "Itor".into(),
            name: String::new(),
            role: CreatorRole::Editor,
        },
    ];
    paper.publication.publisher = Some("Acme Press".into());
    paper.publication.isbn = Some("978-0-13-468599-1".into());
    paper.publication.issn = Some("1234-5678".into());
    paper.publication.series = Some("Great Works".into());
    paper.publication.place = Some("London".into());
    paper.publication.language = Some("en".into());

    let id = db.insert_paper(&paper).await.unwrap();
    let got = db
        .get_papers_by_ids(&[id])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(got.item_type, "book");
    assert_eq!(got.creators.len(), 2);
    assert_eq!(got.creators[0], Creator::author("Ada", "Lovelace"));
    assert_eq!(got.creators[1].role, CreatorRole::Editor);
    // author_names excludes the editor.
    assert_eq!(got.author_names(), vec!["Ada Lovelace"]);
    // Venue fields recovered from extra_meta.
    assert_eq!(got.publication.isbn.as_deref(), Some("978-0-13-468599-1"));
    assert_eq!(got.publication.issn.as_deref(), Some("1234-5678"));
    assert_eq!(got.publication.series.as_deref(), Some("Great Works"));
    assert_eq!(got.publication.place.as_deref(), Some("London"));
    assert_eq!(got.publication.language.as_deref(), Some("en"));
    assert_eq!(got.publication.publisher.as_deref(), Some("Acme Press"));
}

/// A row written before enrichment stores the authors column as an array of
/// display strings and has no item_type; reading it back must not fail and must
/// yield author creators plus the journalArticle default.
#[tokio::test]
async fn legacy_authors_and_missing_item_type_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    // Simulate a legacy row: authors as JSON strings, item_type left at DEFAULT.
    db.conn()
        .execute(
            "INSERT INTO papers (id, title, authors, date_added, date_modified) \
             VALUES ('legacy1', 'Old Paper', '[\"John Doe\", \"Jane Smith\"]', \
             '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            (),
        )
        .await
        .unwrap();

    let got = db
        .get_papers_by_ids(&["legacy1".to_string()])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(got.item_type, "journalArticle");
    assert_eq!(got.author_names(), vec!["John Doe", "Jane Smith"]);
    assert_eq!(got.creators[0], Creator::author("John", "Doe"));
}

/// A pre-existing `citation.extra_meta` object survives alongside the venue
/// payload sharing the same `extra_meta` column.
#[tokio::test]
async fn extra_meta_and_venue_coexist() {
    let dir = tempfile::tempdir().unwrap();
    let db = open_test_db(dir.path()).await;

    let mut paper = Paper::new("Coexist".to_string());
    paper.citation.extra_meta = Some(serde_json::json!({ "source": "crossref" }));
    paper.publication.isbn = Some("978-0-00-000000-0".into());

    let id = db.insert_paper(&paper).await.unwrap();
    let got = db
        .get_papers_by_ids(&[id])
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(got.publication.isbn.as_deref(), Some("978-0-00-000000-0"));
    assert_eq!(
        got.citation.extra_meta,
        Some(serde_json::json!({ "source": "crossref" }))
    );
}

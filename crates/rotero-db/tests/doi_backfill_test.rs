//! Verifies the v12 migration backfills existing `doi` values to their canonical
//! stored form (e.g. `10.48550/arXiv.X` → `arXiv:X`) while leaving plain DOIs and
//! unrecognized values untouched.

use rotero_db::Database;
use rotero_db::turso;

/// Seed a pre-v12 (version 11) papers table with the given `(id, doi)` rows,
/// then run migrations as the app does and read the papers back.
async fn migrate_v11_with_dois(rows: &[(&str, &str)]) -> Vec<rotero_models::Paper> {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("rotero.db");
    let raw = turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();

    conn.execute(
        "CREATE TABLE papers (
            id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '',
            authors TEXT NOT NULL DEFAULT '[]', year INTEGER, doi TEXT,
            abstract_text TEXT, journal TEXT, volume TEXT, issue TEXT, pages TEXT,
            publisher TEXT, url TEXT, pdf_path TEXT,
            date_added TEXT NOT NULL, date_modified TEXT NOT NULL,
            is_favorite INTEGER NOT NULL DEFAULT 0, is_read INTEGER NOT NULL DEFAULT 0,
            extra_meta TEXT, fulltext TEXT, citation_count INTEGER,
            citation_key TEXT, pdf_url TEXT, item_type TEXT NOT NULL DEFAULT 'journalArticle'
        )",
        (),
    )
    .await
    .unwrap();
    conn.execute("CREATE TABLE schema_version (version INTEGER NOT NULL)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO schema_version (version) VALUES (11)", ())
        .await
        .unwrap();

    for (id, doi) in rows {
        conn.execute(
            "INSERT INTO papers (id, title, authors, doi, date_added, date_modified) \
             VALUES (?1, ?2, '[]', ?3, '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            turso::params::Params::Positional(vec![
                turso::Value::Text(id.to_string()),
                turso::Value::Text(format!("Paper {id}")),
                turso::Value::Text(doi.to_string()),
            ]),
        )
        .await
        .unwrap();
    }

    rotero_db::schema::initialize_db(&conn).await.unwrap();
    drop(conn);
    // Read-only: this fixture is a pre-migration database, and attaching avoids
    // initializing CRR metadata that the migration under test does not need.
    let db = Database::attach_readonly(dir.path().to_path_buf())
        .await
        .unwrap();
    let mut papers = db.list_papers().await.unwrap();
    papers.sort_by(|a, b| a.id.cmp(&b.id));
    papers
}

#[tokio::test]
async fn backfills_arxiv_doi_to_canonical_form() {
    let papers = migrate_v11_with_dois(&[
        ("p1", "10.48550/arXiv.1802.06070"), // arXiv-DOI → canonical
        ("p2", "arXiv:2401.11660"),          // already canonical
        ("p3", "10.1038/nature12373"),       // plain DOI → unchanged
        ("p4", "not-an-identifier"),         // unrecognized → unchanged
    ])
    .await;

    let doi = |id: &str| {
        papers
            .iter()
            .find(|p| p.id.as_deref() == Some(id))
            .and_then(|p| p.doi.clone())
    };

    assert_eq!(doi("p1").as_deref(), Some("arXiv:1802.06070"));
    assert_eq!(doi("p2").as_deref(), Some("arXiv:2401.11660"));
    assert_eq!(doi("p3").as_deref(), Some("10.1038/nature12373"));
    assert_eq!(doi("p4").as_deref(), Some("not-an-identifier"));
}

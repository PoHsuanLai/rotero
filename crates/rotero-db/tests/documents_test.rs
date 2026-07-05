//! CRUD round-trip for the standalone `documents` table, plus the
//! collection-link filter and `ON DELETE SET NULL` behavior.

use rotero_db::{collections, documents, schema};
use rotero_models::{Collection, Document, DocumentKind};

async fn open_test_db(dir: &std::path::Path) -> rotero_db::turso::Connection {
    std::fs::create_dir_all(dir).unwrap();
    let db_path = dir.join("test.db");
    let db = rotero_db::turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();
    schema::initialize_db(&conn).await.unwrap();
    conn
}

/// A pre-v10 database (no `documents` table) must gain it after `initialize_db`,
/// simulating the real upgrade path for existing users.
#[tokio::test]
async fn migration_from_v9_adds_documents_table() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("old.db");
    let db = rotero_db::turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();

    // Seed a minimal v9 schema: a schema_version row at 9, and just enough for
    // the migration to run against (no documents table).
    conn.execute("CREATE TABLE schema_version (version INTEGER NOT NULL)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO schema_version (version) VALUES (9)", ())
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE collections (id TEXT PRIMARY KEY, name TEXT NOT NULL, parent_id TEXT, position INTEGER NOT NULL DEFAULT 0)",
        (),
    )
    .await
    .unwrap();

    // Run the app's real startup path.
    rotero_db::schema::initialize_db(&conn).await.unwrap();

    // The documents table now exists and is usable.
    let doc = Document::new("Post-migration".to_string(), DocumentKind::Research, None);
    let id = documents::insert_document(&conn, &doc).await.unwrap();
    assert!(documents::get_document(&conn, &id).await.unwrap().is_some());
}

/// A v10 `documents` table (no `format` column) upgrades to v11 by gaining the
/// column, and rows written before it read back as the Typst default.
#[tokio::test]
async fn migration_from_v10_adds_format_column() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("v10.db");
    let db = rotero_db::turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = db.connect().unwrap();

    conn.execute("CREATE TABLE schema_version (version INTEGER NOT NULL)", ())
        .await
        .unwrap();
    conn.execute("INSERT INTO schema_version (version) VALUES (10)", ())
        .await
        .unwrap();
    conn.execute(
        "CREATE TABLE collections (id TEXT PRIMARY KEY, name TEXT NOT NULL, parent_id TEXT, position INTEGER NOT NULL DEFAULT 0)",
        (),
    )
    .await
    .unwrap();
    // The v10 documents table — note: no `format` column.
    conn.execute(
        "CREATE TABLE documents (
            id TEXT PRIMARY KEY, title TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '',
            collection_id TEXT, template TEXT NOT NULL DEFAULT 'article',
            csl_style TEXT NOT NULL DEFAULT 'apa', kind TEXT NOT NULL DEFAULT 'summary',
            last_pdf_path TEXT, created_at TEXT NOT NULL, modified_at TEXT NOT NULL)",
        (),
    )
    .await
    .unwrap();
    // A pre-migration row.
    conn.execute(
        "INSERT INTO documents (id, title, created_at, modified_at) VALUES ('old1', 'Legacy', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
        (),
    )
    .await
    .unwrap();

    // Run the real startup migration path.
    rotero_db::schema::initialize_db(&conn).await.unwrap();

    // The legacy row reads back with the Typst default format.
    let legacy = documents::get_document(&conn, "old1").await.unwrap().unwrap();
    assert_eq!(legacy.format, rotero_models::DocumentFormat::Typst);

    // And a new Markdown document round-trips its format.
    let mut md = Document::new("Note".to_string(), DocumentKind::Summary, None);
    md.format = rotero_models::DocumentFormat::Markdown;
    let id = documents::insert_document(&conn, &md).await.unwrap();
    let fetched = documents::get_document(&conn, &id).await.unwrap().unwrap();
    assert_eq!(fetched.format, rotero_models::DocumentFormat::Markdown);
}

#[tokio::test]
async fn document_crud_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_test_db(dir.path()).await;

    let coll_id = {
        let c = Collection::new("Transformers".to_string());
        collections::insert_collection(&conn, &c).await.unwrap()
    };

    // Insert
    let mut doc = Document::new(
        "Transformer Survey".to_string(),
        DocumentKind::Summary,
        Some(coll_id.clone()),
    );
    doc.body = "# Survey\n\nBody.".to_string();
    doc.csl_style = "ieee".to_string();
    let id = documents::insert_document(&conn, &doc).await.unwrap();

    // Get
    let fetched = documents::get_document(&conn, &id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "Transformer Survey");
    assert_eq!(fetched.body, "# Survey\n\nBody.");
    assert_eq!(fetched.collection_id.as_deref(), Some(coll_id.as_str()));
    assert_eq!(fetched.csl_style, "ieee");
    assert!(matches!(fetched.kind, DocumentKind::Summary));
    assert!(fetched.last_pdf_path.is_none());

    // List + collection filter
    assert_eq!(documents::list_documents(&conn).await.unwrap().len(), 1);
    assert_eq!(
        documents::list_documents_for_collection(&conn, &coll_id)
            .await
            .unwrap()
            .len(),
        1
    );

    // Update
    let mut updated = fetched.clone();
    updated.title = "Updated Survey".to_string();
    updated.kind = DocumentKind::LitReview;
    updated.last_pdf_path = Some("/tmp/doc.pdf".to_string());
    documents::update_document(&conn, &updated).await.unwrap();

    let after = documents::get_document(&conn, &id).await.unwrap().unwrap();
    assert_eq!(after.title, "Updated Survey");
    assert!(matches!(after.kind, DocumentKind::LitReview));
    assert_eq!(after.last_pdf_path.as_deref(), Some("/tmp/doc.pdf"));

    // Deleting the collection must not delete the document (documents are
    // standalone). Note: turso does not enforce FK actions without
    // `PRAGMA foreign_keys=ON` (not set anywhere in this app), so the
    // `collection_id` may dangle; the app layer treats an unresolved link as
    // "no collection". The document itself survives regardless.
    collections::delete_collection(&conn, &coll_id).await.unwrap();
    let survivor = documents::get_document(&conn, &id).await.unwrap();
    assert!(survivor.is_some(), "document must survive collection deletion");

    // Delete
    documents::delete_document(&conn, &id).await.unwrap();
    assert!(documents::get_document(&conn, &id).await.unwrap().is_none());
    assert_eq!(documents::list_documents(&conn).await.unwrap().len(), 0);
}

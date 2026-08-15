//! CRUD round-trip for the standalone `documents` table, plus the
//! collection-link filter and the schema migrations that add the table and its
//! `format` column.

use rotero_db::Database;
use rotero_models::{Collection, Document, DocumentFormat, DocumentKind};

/// Open a raw turso connection on `<dir>/rotero.db` (the filename
/// [`Database::open`] uses) so a test can seed an old schema before upgrading.
async fn raw_conn(dir: &std::path::Path) -> rotero_db::turso::Connection {
    std::fs::create_dir_all(dir).unwrap();
    let db_path = dir.join("rotero.db");
    let db = rotero_db::turso::Builder::new_local(&db_path.to_string_lossy())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    db.connect().unwrap()
}

#[tokio::test]
async fn document_crud_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    let coll_id = db
        .insert_collection(&Collection::new("Transformers".to_string()))
        .await
        .unwrap();

    // Insert
    let mut doc = Document::new(
        "Transformer Survey".to_string(),
        DocumentKind::Summary,
        Some(coll_id.clone()),
    );
    doc.body = "= Survey\n\nBody.".to_string();
    doc.csl_style = "ieee".to_string();
    let id = db.insert_document(&doc).await.unwrap();

    // Get
    let fetched = db.get_document(&id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "Transformer Survey");
    assert_eq!(fetched.body, "= Survey\n\nBody.");
    assert_eq!(fetched.collection_id.as_deref(), Some(coll_id.as_str()));
    assert_eq!(fetched.csl_style, "ieee");
    assert!(matches!(fetched.kind, DocumentKind::Summary));
    assert_eq!(fetched.format, DocumentFormat::Typst);
    assert!(fetched.last_pdf_path.is_none());

    // List + collection filter
    assert_eq!(db.list_documents().await.unwrap().len(), 1);
    assert_eq!(
        db.list_documents_for_collection(&coll_id).await.unwrap().len(),
        1
    );

    // Update
    let mut updated = fetched.clone();
    updated.title = "Updated Survey".to_string();
    updated.kind = DocumentKind::LitReview;
    updated.format = DocumentFormat::Markdown;
    updated.last_pdf_path = Some("/tmp/doc.pdf".to_string());
    db.update_document(&updated).await.unwrap();

    let after = db.get_document(&id).await.unwrap().unwrap();
    assert_eq!(after.title, "Updated Survey");
    assert!(matches!(after.kind, DocumentKind::LitReview));
    assert_eq!(after.format, DocumentFormat::Markdown);
    assert_eq!(after.last_pdf_path.as_deref(), Some("/tmp/doc.pdf"));

    // Deleting the collection must not delete the document (documents are
    // standalone). Note: turso does not enforce FK actions without
    // `PRAGMA foreign_keys=ON` (not set anywhere in this app), so the
    // `collection_id` may dangle; the app layer treats an unresolved link as
    // "no collection". The document itself survives regardless.
    db.delete_collection(&coll_id).await.unwrap();
    assert!(
        db.get_document(&id).await.unwrap().is_some(),
        "document must survive collection deletion"
    );

    // Delete
    db.delete_document(&id).await.unwrap();
    assert!(db.get_document(&id).await.unwrap().is_none());
    assert_eq!(db.list_documents().await.unwrap().len(), 0);
}

/// A pre-v10 database (no `documents` table) must gain it after the migration
/// runs, simulating the real upgrade path for existing users.
#[tokio::test]
async fn migration_from_v9_adds_documents_table() {
    let dir = tempfile::tempdir().unwrap();
    {
        // Seed a minimal v9 schema on the DB file `Database::open` will reuse.
        let conn = raw_conn(dir.path()).await;
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
    }

    // Open through the real startup path, which runs the migrations.
    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    // The documents table now exists and is usable.
    let doc = Document::new("Post-migration".to_string(), DocumentKind::Research, None);
    let id = db.insert_document(&doc).await.unwrap();
    assert!(db.get_document(&id).await.unwrap().is_some());
}

/// A v10 `documents` table (no `format` column) upgrades by gaining the column,
/// and rows written before it read back as the Typst default.
#[tokio::test]
async fn migration_from_v10_adds_format_column() {
    let dir = tempfile::tempdir().unwrap();
    {
        let conn = raw_conn(dir.path()).await;
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
        conn.execute(
            "INSERT INTO documents (id, title, created_at, modified_at) VALUES ('old1', 'Legacy', '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z')",
            (),
        )
        .await
        .unwrap();
    }

    let db = Database::open(dir.path().to_path_buf()).await.unwrap();

    // The legacy row reads back with the Typst default format.
    let legacy = db.get_document("old1").await.unwrap().unwrap();
    assert_eq!(legacy.format, DocumentFormat::Typst);

    // And a new Markdown document round-trips its format.
    let mut md = Document::new("Note".to_string(), DocumentKind::Summary, None);
    md.format = DocumentFormat::Markdown;
    let id = db.insert_document(&md).await.unwrap();
    let fetched = db.get_document(&id).await.unwrap().unwrap();
    assert_eq!(fetched.format, DocumentFormat::Markdown);
}

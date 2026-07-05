use chrono::Utc;
use rotero_models::{Document, DocumentFormat, DocumentKind};
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

/// Columns tracked for CRR (must match the registry entry in `crr::mod`).
const TRACKED: &[&str] = &[
    "title",
    "body",
    "collection_id",
    "template",
    "csl_style",
    "kind",
    "last_pdf_path",
    "created_at",
    "modified_at",
    "format",
];

fn opt_text(s: &Option<String>) -> Value {
    s.clone().map(Value::Text).unwrap_or(Value::Null)
}

/// Insert a new document and return its generated UUID.
pub async fn insert_document(conn: &Connection, doc: &Document) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    conn.execute(
        queries::DOCUMENT_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(doc.title.clone()),
            Value::Text(doc.body.clone()),
            opt_text(&doc.collection_id),
            Value::Text(doc.template.clone()),
            Value::Text(doc.csl_style.clone()),
            Value::Text(doc.kind.as_str().to_string()),
            opt_text(&doc.last_pdf_path),
            Value::Text(doc.created_at.to_rfc3339()),
            Value::Text(doc.modified_at.to_rfc3339()),
            Value::Text(doc.format.as_str().to_string()),
        ]),
    )
    .await?;

    crr::track_insert(conn, "documents", &uuid, TRACKED).await?;
    Ok(uuid)
}

/// List all documents, newest first.
pub async fn list_documents(conn: &Connection) -> Result<Vec<Document>, turso::Error> {
    let mut rows = conn.query(queries::DOCUMENT_LIST, ()).await?;
    crate::collect_rows(&mut rows).await
}

/// List documents linked to a specific collection, newest first.
pub async fn list_documents_for_collection(
    conn: &Connection,
    collection_id: &str,
) -> Result<Vec<Document>, turso::Error> {
    let mut rows = conn
        .query(
            queries::DOCUMENT_LIST_FOR_COLLECTION,
            [Value::Text(collection_id.to_string())],
        )
        .await?;
    crate::collect_rows(&mut rows).await
}

/// Fetch a single document by ID.
pub async fn get_document(
    conn: &Connection,
    id: &str,
) -> Result<Option<Document>, turso::Error> {
    let mut rows = conn
        .query(queries::DOCUMENT_GET, [Value::Text(id.to_string())])
        .await?;
    let docs: Vec<Document> = crate::collect_rows(&mut rows).await?;
    Ok(docs.into_iter().next())
}

/// Update all editable fields of a document, touching its modified timestamp.
pub async fn update_document(conn: &Connection, doc: &Document) -> Result<(), turso::Error> {
    let id = doc.id.clone().unwrap_or_default();
    conn.execute(
        queries::DOCUMENT_UPDATE,
        turso::params::Params::Positional(vec![
            Value::Text(doc.title.clone()),
            Value::Text(doc.body.clone()),
            opt_text(&doc.collection_id),
            Value::Text(doc.template.clone()),
            Value::Text(doc.csl_style.clone()),
            Value::Text(doc.kind.as_str().to_string()),
            opt_text(&doc.last_pdf_path),
            Value::Text(Utc::now().to_rfc3339()),
            Value::Text(doc.format.as_str().to_string()),
            Value::Text(id.clone()),
        ]),
    )
    .await?;
    crr::track_update(
        conn,
        "documents",
        &id,
        &[
            "title",
            "body",
            "collection_id",
            "template",
            "csl_style",
            "kind",
            "last_pdf_path",
            "modified_at",
            "format",
        ],
    )
    .await?;
    Ok(())
}

/// Delete a document by ID.
pub async fn delete_document(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::DOCUMENT_DELETE, [Value::Text(id.to_string())])
        .await?;
    crr::track_delete(conn, "documents", id).await?;
    Ok(())
}

impl crate::FromRow for Document {
    fn from_row(row: &turso::Row) -> Self {
        let text = |i: usize| row.get_value(i).ok().and_then(|v| v.as_text().cloned());
        let text_or = |i: usize| text(i).unwrap_or_default();
        let parse_dt = |i: usize| {
            chrono::DateTime::parse_from_rfc3339(&text_or(i))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        };

        Document {
            id: text(0),
            title: text_or(1),
            body: text_or(2),
            collection_id: text(3),
            template: {
                let t = text_or(4);
                if t.is_empty() { "article".to_string() } else { t }
            },
            csl_style: {
                let s = text_or(5);
                if s.is_empty() { "apa".to_string() } else { s }
            },
            kind: DocumentKind::from_str_or_default(&text_or(6)),
            last_pdf_path: text(7),
            created_at: parse_dt(8),
            modified_at: parse_dt(9),
            // Column 10; rows written before the v11 migration read as empty and
            // default to Typst.
            format: DocumentFormat::from_str_or_default(&text_or(10)),
        }
    }
}

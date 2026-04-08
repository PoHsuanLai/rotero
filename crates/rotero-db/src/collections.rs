use rotero_models::Collection;
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

/// Insert a new collection and return its generated UUID.
pub async fn insert_collection(
    conn: &Connection,
    coll: &Collection,
) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    conn.execute(
        queries::COLLECTION_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(coll.name.clone()),
            coll.parent_id
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            Value::Integer(coll.position as i64),
        ]),
    )
    .await?;

    crr::track_insert(
        conn,
        "collections",
        &uuid,
        &["name", "parent_id", "position"],
    )
    .await?;

    Ok(uuid)
}

impl crate::FromRow for Collection {
    fn from_row(row: &turso::Row) -> Self {
        Collection {
            id: crate::get_opt_text(row, 0),
            name: crate::get_text(row, 1),
            parent_id: crate::get_opt_text(row, 2),
            position: crate::get_opt_i64(row, 3).unwrap_or(0) as i32,
        }
    }
}

/// List all collections ordered by position.
pub async fn list_collections(conn: &Connection) -> Result<Vec<Collection>, turso::Error> {
    let mut rows = conn.query(queries::COLLECTION_LIST, ()).await?;
    crate::collect_rows(&mut rows).await
}

/// Rename a collection.
pub async fn rename_collection(
    conn: &Connection,
    id: &str,
    name: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_RENAME,
        turso::params::Params::Positional(vec![
            Value::Text(name.to_string()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(conn, "collections", id, &["name"]).await?;
    Ok(())
}

/// Move a collection under a new parent (or to root if `None`).
pub async fn reparent_collection(
    conn: &Connection,
    id: &str,
    new_parent_id: Option<&str>,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_REPARENT,
        turso::params::Params::Positional(vec![
            new_parent_id
                .map(|s| Value::Text(s.to_string()))
                .unwrap_or(Value::Null),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(conn, "collections", id, &["parent_id"]).await?;
    Ok(())
}

/// Delete a collection by ID, cascading to paper memberships.
pub async fn delete_collection(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_DELETE, [Value::Text(id.to_string())])
        .await?;
    crr::track_delete(conn, "collections", id).await?;
    Ok(())
}

/// Return all paper IDs belonging to a collection.
pub async fn list_paper_ids_in_collection(
    conn: &Connection,
    collection_id: &str,
) -> Result<Vec<String>, turso::Error> {
    let mut rows = conn
        .query(
            queries::COLLECTION_PAPER_IDS,
            [Value::Text(collection_id.to_string())],
        )
        .await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Add a paper to a collection (idempotent via INSERT OR IGNORE).
pub async fn add_paper_to_collection(
    conn: &Connection,
    paper_id: &str,
    collection_id: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_ADD_PAPER,
        [
            Value::Text(paper_id.to_string()),
            Value::Text(collection_id.to_string()),
        ],
    )
    .await?;
    let pk = format!("{paper_id}:{collection_id}");
    crr::track_insert(
        conn,
        "paper_collections",
        &pk,
        &["paper_id", "collection_id"],
    )
    .await?;
    Ok(())
}

/// Remove a paper from a collection.
pub async fn remove_paper_from_collection(
    conn: &Connection,
    paper_id: &str,
    collection_id: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_REMOVE_PAPER,
        [
            Value::Text(paper_id.to_string()),
            Value::Text(collection_id.to_string()),
        ],
    )
    .await?;
    let pk = format!("{paper_id}:{collection_id}");
    crr::track_delete(conn, "paper_collections", &pk).await?;
    Ok(())
}

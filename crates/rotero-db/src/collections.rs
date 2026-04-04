#![allow(dead_code)]
use rotero_models::Collection;
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

pub async fn insert_collection(conn: &Connection, coll: &Collection) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    conn.execute(
        queries::COLLECTION_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(coll.name.clone()),
            coll.parent_id.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            Value::Integer(coll.position as i64),
        ]),
    )
    .await?;

    let _ = crr::track_insert(conn, "collections", &uuid, &["name", "parent_id", "position"]).await;

    Ok(uuid)
}

pub async fn list_collections(conn: &Connection) -> Result<Vec<Collection>, turso::Error> {
    let mut rows = conn
        .query(queries::COLLECTION_LIST, ())
        .await?;

    let mut colls = Vec::new();
    while let Some(row) = rows.next().await? {
        colls.push(Collection {
            id: row.get_value(0).ok().and_then(|v| v.as_text().cloned()),
            name: row
                .get_value(1)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_default(),
            parent_id: row.get_value(2).ok().and_then(|v| v.as_text().cloned()),
            position: row
                .get_value(3)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0) as i32,
        });
    }
    Ok(colls)
}

pub async fn rename_collection(conn: &Connection, id: &str, name: &str) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_RENAME,
        turso::params::Params::Positional(vec![Value::Text(name.to_string()), Value::Text(id.to_string())]),
    )
    .await?;
    let _ = crr::track_update(conn, "collections", id, &["name"]).await;
    Ok(())
}

pub async fn reparent_collection(
    conn: &Connection,
    id: &str,
    new_parent_id: Option<&str>,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_REPARENT,
        turso::params::Params::Positional(vec![
            new_parent_id.map(|s| Value::Text(s.to_string())).unwrap_or(Value::Null),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    let _ = crr::track_update(conn, "collections", id, &["parent_id"]).await;
    Ok(())
}

pub async fn delete_collection(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_DELETE, [Value::Text(id.to_string())]).await?;
    let _ = crr::track_delete(conn, "collections", id).await;
    Ok(())
}

pub async fn list_paper_ids_in_collection(
    conn: &Connection,
    collection_id: &str,
) -> Result<Vec<String>, turso::Error> {
    let mut rows = conn
        .query(queries::COLLECTION_PAPER_IDS, [Value::Text(collection_id.to_string())])
        .await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub async fn add_paper_to_collection(
    conn: &Connection,
    paper_id: &str,
    collection_id: &str,
) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_ADD_PAPER, [Value::Text(paper_id.to_string()), Value::Text(collection_id.to_string())])
        .await?;
    let pk = format!("{paper_id}:{collection_id}");
    let _ = crr::track_insert(conn, "paper_collections", &pk, &["paper_id", "collection_id"]).await;
    Ok(())
}

pub async fn remove_paper_from_collection(
    conn: &Connection,
    paper_id: &str,
    collection_id: &str,
) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_REMOVE_PAPER, [Value::Text(paper_id.to_string()), Value::Text(collection_id.to_string())])
        .await?;
    let pk = format!("{paper_id}:{collection_id}");
    let _ = crr::track_delete(conn, "paper_collections", &pk).await;
    Ok(())
}

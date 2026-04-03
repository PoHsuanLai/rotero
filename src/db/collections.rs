#![allow(dead_code)]
use rotero_models::Collection;
use turso::{Connection, Value};

use super::queries;

pub async fn insert_collection(conn: &Connection, coll: &Collection) -> Result<i64, turso::Error> {
    conn.execute(
        queries::COLLECTION_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(coll.name.clone()),
            coll.parent_id.map(Value::Integer).unwrap_or(Value::Null),
            Value::Integer(coll.position as i64),
        ]),
    )
    .await?;

    let mut rows = conn.query(queries::LAST_INSERT_ROWID, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_collections(conn: &Connection) -> Result<Vec<Collection>, turso::Error> {
    let mut rows = conn
        .query(queries::COLLECTION_LIST, ())
        .await?;

    let mut colls = Vec::new();
    while let Some(row) = rows.next().await? {
        colls.push(Collection {
            id: row.get_value(0).ok().and_then(|v| v.as_integer().copied()),
            name: row
                .get_value(1)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_default(),
            parent_id: row.get_value(2).ok().and_then(|v| v.as_integer().copied()),
            position: row
                .get_value(3)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0) as i32,
        });
    }
    Ok(colls)
}

pub async fn rename_collection(conn: &Connection, id: i64, name: &str) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_RENAME,
        turso::params::Params::Positional(vec![Value::Text(name.to_string()), Value::Integer(id)]),
    )
    .await?;
    Ok(())
}

pub async fn reparent_collection(
    conn: &Connection,
    id: i64,
    new_parent_id: Option<i64>,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::COLLECTION_REPARENT,
        turso::params::Params::Positional(vec![
            new_parent_id.map(Value::Integer).unwrap_or(Value::Null),
            Value::Integer(id),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn delete_collection(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_DELETE, [id]).await?;
    Ok(())
}

pub async fn list_paper_ids_in_collection(
    conn: &Connection,
    collection_id: i64,
) -> Result<Vec<i64>, turso::Error> {
    let mut rows = conn
        .query(queries::COLLECTION_PAPER_IDS, [collection_id])
        .await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_integer().copied()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub async fn add_paper_to_collection(
    conn: &Connection,
    paper_id: i64,
    collection_id: i64,
) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_ADD_PAPER, [paper_id, collection_id])
        .await?;
    Ok(())
}

pub async fn remove_paper_from_collection(
    conn: &Connection,
    paper_id: i64,
    collection_id: i64,
) -> Result<(), turso::Error> {
    conn.execute(queries::COLLECTION_REMOVE_PAPER, [paper_id, collection_id])
        .await?;
    Ok(())
}

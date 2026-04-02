#![allow(dead_code)]
use turso::{Connection, Value};
use rotero_models::Collection;

pub async fn insert_collection(conn: &Connection, coll: &Collection) -> Result<i64, turso::Error> {
    conn.execute(
        "INSERT INTO collections (name, parent_id, position) VALUES (?1, ?2, ?3)",
        turso::params::Params::Positional(vec![
            Value::Text(coll.name.clone()),
            coll.parent_id.map(|id| Value::Integer(id)).unwrap_or(Value::Null),
            Value::Integer(coll.position as i64),
        ]),
    )
    .await?;

    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows.next().await?.ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_collections(conn: &Connection) -> Result<Vec<Collection>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, name, parent_id, position FROM collections ORDER BY parent_id NULLS FIRST, position",
            (),
        )
        .await?;

    let mut colls = Vec::new();
    while let Some(row) = rows.next().await? {
        colls.push(Collection {
            id: row.get_value(0).ok().and_then(|v| v.as_integer().copied()),
            name: row.get_value(1).ok().and_then(|v| v.as_text().cloned()).unwrap_or_default(),
            parent_id: row.get_value(2).ok().and_then(|v| v.as_integer().copied()),
            position: row.get_value(3).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as i32,
        });
    }
    Ok(colls)
}

pub async fn rename_collection(conn: &Connection, id: i64, name: &str) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE collections SET name = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![
            Value::Text(name.to_string()),
            Value::Integer(id),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn delete_collection(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM collections WHERE id = ?1", [id]).await?;
    Ok(())
}

pub async fn add_paper_to_collection(conn: &Connection, paper_id: i64, collection_id: i64) -> Result<(), turso::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) VALUES (?1, ?2)",
        [paper_id, collection_id],
    )
    .await?;
    Ok(())
}

use rusqlite::{Connection, params};
use rotero_models::Collection;

pub fn insert_collection(conn: &Connection, coll: &Collection) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO collections (name, parent_id, position) VALUES (?1, ?2, ?3)",
        params![coll.name, coll.parent_id, coll.position],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_collections(conn: &Connection) -> rusqlite::Result<Vec<Collection>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, parent_id, position FROM collections ORDER BY parent_id NULLS FIRST, position",
    )?;
    let colls = stmt
        .query_map([], |row| {
            Ok(Collection {
                id: row.get(0).ok(),
                name: row.get(1)?,
                parent_id: row.get(2)?,
                position: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(colls)
}

pub fn rename_collection(conn: &Connection, id: i64, new_name: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE collections SET name = ?1 WHERE id = ?2",
        params![new_name, id],
    )?;
    Ok(())
}

pub fn delete_collection(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM collections WHERE id = ?1", [id])?;
    Ok(())
}

pub fn add_paper_to_collection(conn: &Connection, paper_id: i64, collection_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) VALUES (?1, ?2)",
        params![paper_id, collection_id],
    )?;
    Ok(())
}

pub fn remove_paper_from_collection(conn: &Connection, paper_id: i64, collection_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM paper_collections WHERE paper_id = ?1 AND collection_id = ?2",
        params![paper_id, collection_id],
    )?;
    Ok(())
}

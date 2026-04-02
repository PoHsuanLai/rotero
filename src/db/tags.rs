use rusqlite::{Connection, params};
use rotero_models::Tag;

pub fn insert_tag(conn: &Connection, tag: &Tag) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO tags (name, color) VALUES (?1, ?2)",
        params![tag.name, tag.color],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_or_create_tag(conn: &Connection, name: &str, color: Option<&str>) -> rusqlite::Result<i64> {
    if let Ok(id) = conn.query_row(
        "SELECT id FROM tags WHERE name = ?1",
        [name],
        |row| row.get::<_, i64>(0),
    ) {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO tags (name, color) VALUES (?1, ?2)",
        params![name, color],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_tags(conn: &Connection) -> rusqlite::Result<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name, color FROM tags ORDER BY name")?;
    let tags = stmt
        .query_map([], |row| {
            Ok(Tag {
                id: row.get(0).ok(),
                name: row.get(1)?,
                color: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tags)
}

pub fn tags_for_paper(conn: &Connection, paper_id: i64) -> rusqlite::Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.color FROM tags t
         JOIN paper_tags pt ON t.id = pt.tag_id
         WHERE pt.paper_id = ?1
         ORDER BY t.name",
    )?;
    let tags = stmt
        .query_map([paper_id], |row| {
            Ok(Tag {
                id: row.get(0).ok(),
                name: row.get(1)?,
                color: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(tags)
}

pub fn add_tag_to_paper(conn: &Connection, paper_id: i64, tag_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)",
        params![paper_id, tag_id],
    )?;
    Ok(())
}

pub fn remove_tag_from_paper(conn: &Connection, paper_id: i64, tag_id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM paper_tags WHERE paper_id = ?1 AND tag_id = ?2",
        params![paper_id, tag_id],
    )?;
    Ok(())
}

pub fn delete_tag(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
    Ok(())
}

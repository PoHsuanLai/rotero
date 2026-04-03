use rotero_models::Tag;
use turso::{Connection, Value};

pub async fn get_or_create_tag(
    conn: &Connection,
    name: &str,
    color: Option<&str>,
) -> Result<i64, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id FROM tags WHERE name = ?1",
            [Value::Text(name.to_string())],
        )
        .await?;
    if let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO tags (name, color) VALUES (?1, ?2)",
        turso::params::Params::Positional(vec![
            Value::Text(name.to_string()),
            color
                .map(|c| Value::Text(c.to_string()))
                .unwrap_or(Value::Null),
        ]),
    )
    .await?;
    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_tags(conn: &Connection) -> Result<Vec<Tag>, turso::Error> {
    let mut rows = conn
        .query("SELECT id, name, color FROM tags ORDER BY name", ())
        .await?;
    let mut tags = Vec::new();
    while let Some(row) = rows.next().await? {
        tags.push(Tag {
            id: row.get_value(0).ok().and_then(|v| v.as_integer().copied()),
            name: row
                .get_value(1)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_default(),
            color: row.get_value(2).ok().and_then(|v| v.as_text().cloned()),
        });
    }
    Ok(tags)
}

pub async fn add_tag_to_paper(
    conn: &Connection,
    paper_id: i64,
    tag_id: i64,
) -> Result<(), turso::Error> {
    conn.execute(
        "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)",
        [paper_id, tag_id],
    )
    .await?;
    Ok(())
}

pub async fn rename_tag(conn: &Connection, id: i64, name: &str) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE tags SET name = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![Value::Text(name.to_string()), Value::Integer(id)]),
    )
    .await?;
    Ok(())
}

pub async fn update_tag_color(conn: &Connection, id: i64, color: &str) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE tags SET color = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![Value::Text(color.to_string()), Value::Integer(id)]),
    )
    .await?;
    Ok(())
}

pub async fn list_paper_ids_by_tag(
    conn: &Connection,
    tag_id: i64,
) -> Result<Vec<i64>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT paper_id FROM paper_tags WHERE tag_id = ?1",
            [tag_id],
        )
        .await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_integer().copied()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub async fn delete_tag(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM tags WHERE id = ?1", [id]).await?;
    Ok(())
}

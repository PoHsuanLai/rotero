use rotero_models::Tag;
use turso::{Connection, Value};

use super::queries;

pub async fn get_or_create_tag(
    conn: &Connection,
    name: &str,
    color: Option<&str>,
) -> Result<i64, turso::Error> {
    let mut rows = conn
        .query(queries::TAG_FIND_BY_NAME, [Value::Text(name.to_string())])
        .await?;
    if let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
        return Ok(id);
    }
    // Auto-assign a color from the palette if none provided
    let actual_color = color.map(|c| c.to_string()).unwrap_or_else(|| {
        const PALETTE: &[&str] = &[
            "#6b7085", "#7c6b85", "#6b8580", "#857a6b",
            "#6b7a85", "#856b7a", "#6b856e", "#85706b",
            "#6e6b85", "#7a856b", "#856b6b", "#6b8585",
        ];
        // Use a hash of the name to pick a color deterministically
        let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
        PALETTE[hash % PALETTE.len()].to_string()
    });
    conn.execute(
        queries::TAG_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(name.to_string()),
            Value::Text(actual_color),
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

pub async fn list_tags(conn: &Connection) -> Result<Vec<Tag>, turso::Error> {
    let mut rows = conn.query(queries::TAG_LIST, ()).await?;
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
    conn.execute(queries::TAG_ADD_TO_PAPER, [paper_id, tag_id])
        .await?;
    Ok(())
}

pub async fn rename_tag(conn: &Connection, id: i64, name: &str) -> Result<(), turso::Error> {
    conn.execute(
        queries::TAG_RENAME,
        turso::params::Params::Positional(vec![Value::Text(name.to_string()), Value::Integer(id)]),
    )
    .await?;
    Ok(())
}

pub async fn update_tag_color(conn: &Connection, id: i64, color: &str) -> Result<(), turso::Error> {
    conn.execute(
        queries::TAG_UPDATE_COLOR,
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
        .query(queries::TAG_PAPER_IDS, [tag_id])
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
    conn.execute(queries::TAG_DELETE, [id]).await?;
    Ok(())
}

use rotero_models::Tag;
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

pub async fn get_or_create_tag(
    conn: &Connection,
    name: &str,
    color: Option<&str>,
) -> Result<String, turso::Error> {
    let mut rows = conn
        .query(queries::TAG_FIND_BY_NAME, [Value::Text(name.to_string())])
        .await?;
    if let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
        return Ok(id);
    }
    let actual_color = color.map(|c| c.to_string()).unwrap_or_else(|| {
        const PALETTE: &[&str] = &[
            "#6b7085", "#7c6b85", "#6b8580", "#857a6b", "#6b7a85", "#856b7a", "#6b856e", "#85706b",
            "#6e6b85", "#7a856b", "#856b6b", "#6b8585",
        ];
        // Deterministic color from name hash
        let hash = name
            .bytes()
            .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
        PALETTE[hash % PALETTE.len()].to_string()
    });
    let uuid = uuid::Uuid::now_v7().to_string();
    conn.execute(
        queries::TAG_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(name.to_string()),
            Value::Text(actual_color),
        ]),
    )
    .await?;
    crr::track_insert(conn, "tags", &uuid, &["name", "color"]).await?;
    Ok(uuid)
}

pub async fn list_tags(conn: &Connection) -> Result<Vec<Tag>, turso::Error> {
    let mut rows = conn.query(queries::TAG_LIST, ()).await?;
    let mut tags = Vec::new();
    while let Some(row) = rows.next().await? {
        tags.push(Tag {
            id: row.get_value(0).ok().and_then(|v| v.as_text().cloned()),
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
    paper_id: &str,
    tag_id: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::TAG_ADD_TO_PAPER,
        [
            Value::Text(paper_id.to_string()),
            Value::Text(tag_id.to_string()),
        ],
    )
    .await?;
    let pk = format!("{paper_id}:{tag_id}");
    crr::track_insert(conn, "paper_tags", &pk, &["paper_id", "tag_id"]).await?;
    Ok(())
}

pub async fn rename_tag(conn: &Connection, id: &str, name: &str) -> Result<(), turso::Error> {
    conn.execute(
        queries::TAG_RENAME,
        turso::params::Params::Positional(vec![
            Value::Text(name.to_string()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(conn, "tags", id, &["name"]).await?;
    Ok(())
}

pub async fn update_tag_color(
    conn: &Connection,
    id: &str,
    color: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::TAG_UPDATE_COLOR,
        turso::params::Params::Positional(vec![
            Value::Text(color.to_string()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(conn, "tags", id, &["color"]).await?;
    Ok(())
}

pub async fn list_paper_ids_by_tag(
    conn: &Connection,
    tag_id: &str,
) -> Result<Vec<String>, turso::Error> {
    let mut rows = conn
        .query(queries::TAG_PAPER_IDS, [Value::Text(tag_id.to_string())])
        .await?;
    let mut ids = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
            ids.push(id);
        }
    }
    Ok(ids)
}

pub async fn delete_tag(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::TAG_DELETE, [Value::Text(id.to_string())])
        .await?;
    crr::track_delete(conn, "tags", id).await?;
    Ok(())
}

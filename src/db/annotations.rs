use chrono::Utc;
use turso::{Connection, Value};
use rotero_models::{Annotation, AnnotationType};

pub async fn insert_annotation(conn: &Connection, ann: &Annotation) -> Result<i64, turso::Error> {
    let ann_type_str = match ann.ann_type {
        AnnotationType::Highlight => "highlight",
        AnnotationType::Note => "note",
        AnnotationType::Area => "area",
    };
    let geometry = serde_json::to_string(&ann.geometry).unwrap_or_else(|_| "{}".to_string());

    conn.execute(
        "INSERT INTO annotations (paper_id, page, ann_type, color, content, geometry, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        turso::params::Params::Positional(vec![
            Value::Integer(ann.paper_id),
            Value::Integer(ann.page as i64),
            Value::Text(ann_type_str.to_string()),
            Value::Text(ann.color.clone()),
            ann.content.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            Value::Text(geometry),
            Value::Text(ann.created_at.to_rfc3339()),
            Value::Text(ann.modified_at.to_rfc3339()),
        ]),
    )
    .await?;

    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows.next().await?.ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_annotations_for_paper(conn: &Connection, paper_id: i64) -> Result<Vec<Annotation>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at
             FROM annotations WHERE paper_id = ?1 ORDER BY page, created_at",
            [paper_id],
        )
        .await?;

    let mut anns = Vec::new();
    while let Some(row) = rows.next().await? {
        anns.push(row_to_annotation(&row));
    }
    Ok(anns)
}

pub async fn update_annotation_content(conn: &Connection, id: i64, content: Option<&str>) -> Result<(), turso::Error> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE annotations SET content = ?1, modified_at = ?2 WHERE id = ?3",
        turso::params::Params::Positional(vec![
            content.map(|s| Value::Text(s.to_string())).unwrap_or(Value::Null),
            Value::Text(now),
            Value::Integer(id),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn delete_annotation(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM annotations WHERE id = ?1", [id]).await?;
    Ok(())
}

fn parse_ann_type(s: &str) -> AnnotationType {
    match s {
        "highlight" => AnnotationType::Highlight,
        "note" => AnnotationType::Note,
        "area" => AnnotationType::Area,
        _ => AnnotationType::Note,
    }
}

fn row_to_annotation(row: &turso::Row) -> Annotation {
    let ann_type_str = row.get_value(3).ok().and_then(|v| v.as_text().cloned()).unwrap_or_default();
    let geometry_str = row.get_value(6).ok().and_then(|v| v.as_text().cloned()).unwrap_or_else(|| "{}".to_string());
    let created_str = row.get_value(7).ok().and_then(|v| v.as_text().cloned()).unwrap_or_default();
    let modified_str = row.get_value(8).ok().and_then(|v| v.as_text().cloned()).unwrap_or_default();

    Annotation {
        id: row.get_value(0).ok().and_then(|v| v.as_integer().copied()),
        paper_id: row.get_value(1).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0),
        page: row.get_value(2).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as i32,
        ann_type: parse_ann_type(&ann_type_str),
        color: row.get_value(4).ok().and_then(|v| v.as_text().cloned()).unwrap_or_else(|| "#ffff00".to_string()),
        content: row.get_value(5).ok().and_then(|v| v.as_text().cloned()),
        geometry: serde_json::from_str(&geometry_str).unwrap_or(serde_json::json!({})),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        modified_at: chrono::DateTime::parse_from_rfc3339(&modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

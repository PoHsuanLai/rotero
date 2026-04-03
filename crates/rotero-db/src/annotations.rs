use chrono::Utc;
use rotero_models::{Annotation, AnnotationType};
use turso::{Connection, Value};

use crate::queries;

pub async fn insert_annotation(conn: &Connection, ann: &Annotation) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    let ann_type_str = match ann.ann_type {
        AnnotationType::Highlight => "highlight",
        AnnotationType::Note => "note",
        AnnotationType::Area => "area",
        AnnotationType::Underline => "underline",
        AnnotationType::Ink => "ink",
        AnnotationType::Text => "text",
    };
    let geometry = serde_json::to_string(&ann.geometry).unwrap_or_else(|_| "{}".to_string());

    conn.execute(
        queries::ANNOTATION_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(ann.paper_id.clone()),
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

    Ok(uuid)
}

pub async fn list_annotations_for_paper(
    conn: &Connection,
    paper_id: &str,
) -> Result<Vec<Annotation>, turso::Error> {
    let mut rows = conn
        .query(queries::ANNOTATION_LIST_FOR_PAPER, [Value::Text(paper_id.to_string())])
        .await?;

    let mut anns = Vec::new();
    while let Some(row) = rows.next().await? {
        anns.push(row_to_annotation(&row));
    }
    Ok(anns)
}

pub async fn update_annotation_content(
    conn: &Connection,
    id: &str,
    content: Option<&str>,
) -> Result<(), turso::Error> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        queries::ANNOTATION_UPDATE_CONTENT,
        turso::params::Params::Positional(vec![
            content
                .map(|s| Value::Text(s.to_string()))
                .unwrap_or(Value::Null),
            Value::Text(now),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn update_annotation_color(
    conn: &Connection,
    id: &str,
    color: &str,
) -> Result<(), turso::Error> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        queries::ANNOTATION_UPDATE_COLOR,
        turso::params::Params::Positional(vec![
            Value::Text(color.to_string()),
            Value::Text(now),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn delete_annotation(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::ANNOTATION_DELETE, [Value::Text(id.to_string())]).await?;
    Ok(())
}

fn parse_ann_type(s: &str) -> AnnotationType {
    match s {
        "highlight" => AnnotationType::Highlight,
        "note" => AnnotationType::Note,
        "area" => AnnotationType::Area,
        "underline" => AnnotationType::Underline,
        "ink" => AnnotationType::Ink,
        "text" => AnnotationType::Text,
        _ => AnnotationType::Note,
    }
}

fn row_to_annotation(row: &turso::Row) -> Annotation {
    let ann_type_str = row
        .get_value(3)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let geometry_str = row
        .get_value(6)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_else(|| "{}".to_string());
    let created_str = row
        .get_value(7)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let modified_str = row
        .get_value(8)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();

    Annotation {
        id: row.get_value(0).ok().and_then(|v| v.as_text().cloned()),
        paper_id: row
            .get_value(1)
            .ok()
            .and_then(|v| v.as_text().cloned())
            .unwrap_or_default(),
        page: row
            .get_value(2)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0) as i32,
        ann_type: parse_ann_type(&ann_type_str),
        color: row
            .get_value(4)
            .ok()
            .and_then(|v| v.as_text().cloned())
            .unwrap_or_else(|| "#ffff00".to_string()),
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

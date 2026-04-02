use chrono::Utc;
use rusqlite::{Connection, params};
use rotero_models::{Annotation, AnnotationType};

pub fn insert_annotation(conn: &Connection, ann: &Annotation) -> rusqlite::Result<i64> {
    let ann_type_str = match ann.ann_type {
        AnnotationType::Highlight => "highlight",
        AnnotationType::Note => "note",
        AnnotationType::Area => "area",
    };
    let geometry = serde_json::to_string(&ann.geometry).unwrap_or_else(|_| "{}".to_string());

    conn.execute(
        "INSERT INTO annotations (paper_id, page, ann_type, color, content, geometry, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            ann.paper_id,
            ann.page,
            ann_type_str,
            ann.color,
            ann.content,
            geometry,
            ann.created_at.to_rfc3339(),
            ann.modified_at.to_rfc3339(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_annotations_for_paper(conn: &Connection, paper_id: i64) -> rusqlite::Result<Vec<Annotation>> {
    let mut stmt = conn.prepare(
        "SELECT id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at
         FROM annotations WHERE paper_id = ?1 ORDER BY page, created_at",
    )?;
    let anns = stmt
        .query_map([paper_id], |row| Ok(row_to_annotation(row)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(anns)
}

pub fn update_annotation_content(conn: &Connection, id: i64, content: Option<&str>) -> rusqlite::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE annotations SET content = ?1, modified_at = ?2 WHERE id = ?3",
        params![content, now, id],
    )?;
    Ok(())
}

pub fn update_annotation_color(conn: &Connection, id: i64, color: &str) -> rusqlite::Result<()> {
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE annotations SET color = ?1, modified_at = ?2 WHERE id = ?3",
        params![color, now, id],
    )?;
    Ok(())
}

pub fn delete_annotation(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM annotations WHERE id = ?1", [id])?;
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

fn row_to_annotation(row: &rusqlite::Row) -> Annotation {
    let ann_type_str: String = row.get(3).unwrap_or_default();
    let geometry_str: String = row.get(6).unwrap_or_else(|_| "{}".to_string());
    let created_str: String = row.get(7).unwrap_or_default();
    let modified_str: String = row.get(8).unwrap_or_default();

    Annotation {
        id: row.get(0).ok(),
        paper_id: row.get(1).unwrap_or(0),
        page: row.get(2).unwrap_or(0),
        ann_type: parse_ann_type(&ann_type_str),
        color: row.get(4).unwrap_or_else(|_| "#ffff00".to_string()),
        content: row.get(5).unwrap_or(None),
        geometry: serde_json::from_str(&geometry_str).unwrap_or(serde_json::json!({})),
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        modified_at: chrono::DateTime::parse_from_rfc3339(&modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

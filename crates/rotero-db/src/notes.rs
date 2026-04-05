use chrono::Utc;
use rotero_models::Note;
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

pub async fn insert_note(conn: &Connection, note: &Note) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    conn.execute(
        queries::NOTE_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(note.paper_id.clone()),
            Value::Text(note.title.clone()),
            Value::Text(note.body.clone()),
            Value::Text(note.created_at.to_rfc3339()),
            Value::Text(note.modified_at.to_rfc3339()),
        ]),
    )
    .await?;

    let _ = crr::track_insert(
        conn,
        "notes",
        &uuid,
        &["paper_id", "title", "body", "created_at", "modified_at"],
    )
    .await;

    Ok(uuid)
}

pub async fn list_notes_for_paper(
    conn: &Connection,
    paper_id: &str,
) -> Result<Vec<Note>, turso::Error> {
    let mut rows = conn
        .query(
            queries::NOTE_LIST_FOR_PAPER,
            [Value::Text(paper_id.to_string())],
        )
        .await?;
    let mut notes = Vec::new();
    while let Some(row) = rows.next().await? {
        notes.push(row_to_note(&row));
    }
    Ok(notes)
}

#[allow(dead_code)]
pub async fn update_note(
    conn: &Connection,
    id: &str,
    title: &str,
    body: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::NOTE_UPDATE,
        turso::params::Params::Positional(vec![
            Value::Text(title.to_string()),
            Value::Text(body.to_string()),
            Value::Text(Utc::now().to_rfc3339()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    let _ = crr::track_update(conn, "notes", id, &["title", "body", "modified_at"]).await;
    Ok(())
}

pub async fn delete_note(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::NOTE_DELETE, [Value::Text(id.to_string())])
        .await?;
    let _ = crr::track_delete(conn, "notes", id).await;
    Ok(())
}

fn row_to_note(row: &turso::Row) -> Note {
    let id = row.get_value(0).ok().and_then(|v| v.as_text().cloned());
    let paper_id = row
        .get_value(1)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let title = row
        .get_value(2)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let body = row
        .get_value(3)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let created_str = row
        .get_value(4)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let modified_str = row
        .get_value(5)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();

    Note {
        id,
        paper_id,
        title,
        body,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        modified_at: chrono::DateTime::parse_from_rfc3339(&modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

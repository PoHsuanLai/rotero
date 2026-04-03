use chrono::Utc;
use rotero_models::Note;
use turso::{Connection, Value};

pub async fn insert_note(conn: &Connection, note: &Note) -> Result<i64, turso::Error> {
    conn.execute(
        "INSERT INTO notes (paper_id, title, body, created_at, modified_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        turso::params::Params::Positional(vec![
            Value::Integer(note.paper_id),
            Value::Text(note.title.clone()),
            Value::Text(note.body.clone()),
            Value::Text(note.created_at.to_rfc3339()),
            Value::Text(note.modified_at.to_rfc3339()),
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

pub async fn list_notes_for_paper(
    conn: &Connection,
    paper_id: i64,
) -> Result<Vec<Note>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, paper_id, title, body, created_at, modified_at \
             FROM notes WHERE paper_id = ?1 ORDER BY created_at DESC",
            [Value::Integer(paper_id)],
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
    id: i64,
    title: &str,
    body: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE notes SET title = ?1, body = ?2, modified_at = ?3 WHERE id = ?4",
        turso::params::Params::Positional(vec![
            Value::Text(title.to_string()),
            Value::Text(body.to_string()),
            Value::Text(Utc::now().to_rfc3339()),
            Value::Integer(id),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn delete_note(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM notes WHERE id = ?1", [id])
        .await?;
    Ok(())
}

fn row_to_note(row: &turso::Row) -> Note {
    let id = row
        .get_value(0)
        .ok()
        .and_then(|v| v.as_integer().copied());
    let paper_id = row
        .get_value(1)
        .ok()
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(0);
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

use chrono::Utc;
use rotero_models::Note;
use turso::Value;

use crate::Database;
use crate::queries;

impl Database {
    /// Insert a new note and return its generated UUID.
    pub async fn insert_note(&self, note: &Note) -> Result<String, crate::DbError> {
        let conn = self.conn();
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

        self.touch("notes", crate::clock::Pk::Single(&uuid)).await?;

        Ok(uuid)
    }

    /// List all notes belonging to a given paper.
    pub async fn list_notes_for_paper(&self, paper_id: &str) -> Result<Vec<Note>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(
                queries::NOTE_LIST_FOR_PAPER,
                [Value::Text(paper_id.to_string())],
            )
            .await?;
        Ok(crate::collect_rows(&mut rows).await?)
    }

    /// Update a note's title and body, touching its modified timestamp.
    pub async fn update_note(
        &self,
        id: &str,
        title: &str,
        body: &str,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
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
        self.touch("notes", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Delete a note by ID.
    pub async fn delete_note(&self, id: &str) -> Result<(), crate::DbError> {
        self.tombstone("notes", crate::clock::Pk::Single(id))
            .await?;
        Ok(())
    }
}

impl crate::FromRow for Note {
    fn from_row(row: &turso::Row) -> Self {
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
}

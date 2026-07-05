use rotero_models::Tag;
use turso::Value;

use crate::queries;
use crate::Database;

impl Database {
    /// Find a tag by name, or create it with the given (or auto-generated) color. Returns its UUID.
    pub async fn get_or_create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<String, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(queries::TAG_FIND_BY_NAME, [Value::Text(name.to_string())])
            .await?;
        if let Some(row) = rows.next().await? {
            let id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
            return Ok(id);
        }
        let actual_color = color.map(|c| c.to_string()).unwrap_or_else(|| {
            const PALETTE: &[&str] = &[
                "#6b7085", "#7c6b85", "#6b8580", "#857a6b", "#6b7a85", "#856b7a", "#6b856e",
                "#85706b", "#6e6b85", "#7a856b", "#856b6b", "#6b8585",
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
        self.crr()
            .track_insert("tags", &uuid, &["name", "color"])
            .await?;
        Ok(uuid)
    }

    /// List all tags.
    pub async fn list_tags(&self) -> Result<Vec<Tag>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::TAG_LIST, ()).await?;
        crate::collect_rows(&mut rows).await.map_err(Into::into)
    }

    /// Associate a tag with a paper.
    pub async fn add_tag_to_paper(
        &self,
        paper_id: &str,
        tag_id: &str,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::TAG_ADD_TO_PAPER,
            [
                Value::Text(paper_id.to_string()),
                Value::Text(tag_id.to_string()),
            ],
        )
        .await?;
        let pk = format!("{paper_id}:{tag_id}");
        self.crr()
            .track_insert("paper_tags", &pk, &["paper_id", "tag_id"])
            .await?;
        Ok(())
    }

    /// Rename a tag.
    pub async fn rename_tag(&self, id: &str, name: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::TAG_RENAME,
            turso::params::Params::Positional(vec![
                Value::Text(name.to_string()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr().track_update("tags", id, &["name"]).await?;
        Ok(())
    }

    /// Change a tag's display color.
    pub async fn update_tag_color(&self, id: &str, color: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::TAG_UPDATE_COLOR,
            turso::params::Params::Positional(vec![
                Value::Text(color.to_string()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr().track_update("tags", id, &["color"]).await?;
        Ok(())
    }

    /// Return all paper IDs that have a given tag.
    pub async fn list_paper_ids_by_tag(&self, tag_id: &str) -> Result<Vec<String>, crate::DbError> {
        let conn = self.conn();
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

    /// Delete a tag by ID, cascading to paper-tag associations.
    pub async fn delete_tag(&self, id: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(queries::TAG_DELETE, [Value::Text(id.to_string())])
            .await?;
        self.crr().track_delete("tags", id).await?;
        Ok(())
    }
}

impl crate::FromRow for Tag {
    fn from_row(row: &turso::Row) -> Self {
        Tag {
            id: crate::get_opt_text(row, 0),
            name: crate::get_text(row, 1),
            color: crate::get_opt_text(row, 2),
        }
    }
}

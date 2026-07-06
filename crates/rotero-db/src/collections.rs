use rotero_models::Collection;
use turso::Value;

use crate::Database;
use crate::crr::{Collections, PaperCollections};
use crate::queries;

impl Database {
    /// Insert a new collection and return its generated UUID.
    pub async fn insert_collection(&self, coll: &Collection) -> Result<String, crate::DbError> {
        let conn = self.conn();
        let uuid = uuid::Uuid::now_v7().to_string();
        conn.execute(
            queries::COLLECTION_INSERT,
            turso::params::Params::Positional(vec![
                Value::Text(uuid.clone()),
                Value::Text(coll.name.clone()),
                coll.parent_id
                    .as_ref()
                    .map(|s| Value::Text(s.clone()))
                    .unwrap_or(Value::Null),
                Value::Integer(coll.position as i64),
            ]),
        )
        .await?;

        self.crr()
            .track_insert("collections", &uuid, Collections::ALL)
            .await?;

        Ok(uuid)
    }

    /// List all collections ordered by position.
    pub async fn list_collections(&self) -> Result<Vec<Collection>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::COLLECTION_LIST, ()).await?;
        crate::collect_rows(&mut rows).await.map_err(Into::into)
    }

    /// Rename a collection.
    pub async fn rename_collection(&self, id: &str, name: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::COLLECTION_RENAME,
            turso::params::Params::Positional(vec![
                Value::Text(name.to_string()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr()
            .track_update("collections", id, &[Collections::NAME])
            .await?;
        Ok(())
    }

    /// Move a collection under a new parent (or to root if `None`).
    pub async fn reparent_collection(
        &self,
        id: &str,
        new_parent_id: Option<&str>,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::COLLECTION_REPARENT,
            turso::params::Params::Positional(vec![
                new_parent_id
                    .map(|s| Value::Text(s.to_string()))
                    .unwrap_or(Value::Null),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr()
            .track_update("collections", id, &[Collections::PARENT_ID])
            .await?;
        Ok(())
    }

    /// Delete a collection by ID, cascading to paper memberships.
    pub async fn delete_collection(&self, id: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(queries::COLLECTION_DELETE, [Value::Text(id.to_string())])
            .await?;
        self.crr().track_delete("collections", id).await?;
        Ok(())
    }

    /// Return all paper IDs belonging to a collection.
    pub async fn list_paper_ids_in_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(
                queries::COLLECTION_PAPER_IDS,
                [Value::Text(collection_id.to_string())],
            )
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Return all paper IDs in a collection *and all of its descendant collections*,
    /// deduplicated. Viewing a parent collection aggregates its whole subtree.
    ///
    /// turso does not yet support `WITH RECURSIVE` (see COMPAT.md / tursodatabase
    /// issue #6205), so the descendant walk is done in Rust via [`descendant_ids`].
    /// When recursive CTEs land, delete `descendant_ids` and replace this body with
    /// a single query:
    ///
    /// ```sql
    /// WITH RECURSIVE subtree(id) AS (
    ///     SELECT id FROM collections WHERE id = ?1
    ///     UNION
    ///     SELECT c.id FROM collections c JOIN subtree s ON c.parent_id = s.id
    /// )
    /// SELECT DISTINCT pc.paper_id
    /// FROM paper_collections pc JOIN subtree ON pc.collection_id = subtree.id
    /// ```
    pub async fn list_paper_ids_in_subtree(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        let conn = self.conn();
        let subtree = descendant_ids(conn, collection_id).await?;
        if subtree.is_empty() {
            return Ok(Vec::new());
        }

        // SELECT DISTINCT paper_id FROM paper_collections WHERE collection_id IN (?, ?, ...)
        let placeholders = std::iter::repeat_n("?", subtree.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT DISTINCT paper_id FROM paper_collections WHERE collection_id IN ({placeholders})"
        );
        let params: Vec<Value> = subtree.into_iter().map(Value::Text).collect();

        let mut rows = conn
            .query(&sql, turso::params::Params::Positional(params))
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(id) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Add a paper to a collection (idempotent via INSERT OR IGNORE).
    pub async fn add_paper_to_collection(
        &self,
        paper_id: &str,
        collection_id: &str,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::COLLECTION_ADD_PAPER,
            [
                Value::Text(paper_id.to_string()),
                Value::Text(collection_id.to_string()),
            ],
        )
        .await?;
        let pk = format!("{paper_id}:{collection_id}");
        self.crr()
            .track_insert("paper_collections", &pk, PaperCollections::ALL)
            .await?;
        Ok(())
    }

    /// Remove a paper from a collection.
    pub async fn remove_paper_from_collection(
        &self,
        paper_id: &str,
        collection_id: &str,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::COLLECTION_REMOVE_PAPER,
            [
                Value::Text(paper_id.to_string()),
                Value::Text(collection_id.to_string()),
            ],
        )
        .await?;
        let pk = format!("{paper_id}:{collection_id}");
        self.crr().track_delete("paper_collections", &pk).await?;
        Ok(())
    }
}

impl crate::FromRow for Collection {
    fn from_row(row: &turso::Row) -> Self {
        Collection {
            id: crate::get_opt_text(row, 0),
            name: crate::get_text(row, 1),
            parent_id: crate::get_opt_text(row, 2),
            position: crate::get_opt_i64(row, 3).unwrap_or(0) as i32,
        }
    }
}

/// Collect `collection_id` and every collection transitively parented under it.
///
/// Workaround for turso's missing `WITH RECURSIVE`: loads the flat collection
/// list once and walks the `parent_id` tree in memory. A `visited` set guards
/// against parent-pointer cycles so malformed data can't loop forever.
async fn descendant_ids(
    conn: &turso::Connection,
    root: &str,
) -> Result<Vec<String>, crate::DbError> {
    let mut rows = conn.query(queries::COLLECTION_LIST, ()).await?;
    let all: Vec<Collection> = crate::collect_rows(&mut rows).await?;

    // Build parent -> children adjacency.
    let mut children: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for coll in &all {
        if let (Some(id), Some(parent)) = (coll.id.as_ref(), coll.parent_id.as_ref()) {
            children.entry(parent.clone()).or_default().push(id.clone());
        }
    }

    let mut visited = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![root.to_string()];
    while let Some(id) = stack.pop() {
        if !visited.insert(id.clone()) {
            continue; // already seen — skip to break any cycle
        }
        if let Some(kids) = children.get(&id) {
            stack.extend(kids.iter().cloned());
        }
        out.push(id);
    }
    Ok(out)
}

use crate::queries;
use crate::Database;

impl Database {
    /// Return all (paper_id, tag_id) pairs from the paper_tags junction table.
    pub async fn list_all_paper_tags(&self) -> Result<Vec<(String, String)>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::GRAPH_ALL_PAPER_TAGS, ()).await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            let paper_id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
            let tag_id = row.get_value(1)?.as_text().cloned().unwrap_or_default();
            pairs.push((paper_id, tag_id));
        }
        Ok(pairs)
    }

    /// Return all (paper_id, collection_id) pairs from the paper_collections junction table.
    pub async fn list_all_paper_collections(&self) -> Result<Vec<(String, String)>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::GRAPH_ALL_PAPER_COLLECTIONS, ()).await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            let paper_id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
            let coll_id = row.get_value(1)?.as_text().cloned().unwrap_or_default();
            pairs.push((paper_id, coll_id));
        }
        Ok(pairs)
    }
}

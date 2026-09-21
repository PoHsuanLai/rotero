use turso::Value;

use crate::Database;
use crate::queries;

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
    pub async fn list_all_paper_collections(
        &self,
    ) -> Result<Vec<(String, String)>, crate::DbError> {
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

    /// Return all directed (citing_paper_id, cited_paper_id) citation edges.
    pub async fn list_all_citations(&self) -> Result<Vec<(String, String)>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::GRAPH_ALL_CITATIONS, ()).await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            let citing = row.get_value(0)?.as_text().cloned().unwrap_or_default();
            let cited = row.get_value(1)?.as_text().cloned().unwrap_or_default();
            pairs.push((citing, cited));
        }
        Ok(pairs)
    }

    /// Papers that `paper_id` cites (outgoing).
    pub async fn list_cited_by_paper(&self, paper_id: &str) -> Result<Vec<String>, crate::DbError> {
        self.list_id_column(queries::GRAPH_CITED_BY_PAPER, paper_id)
            .await
    }

    /// Papers that cite `paper_id` (incoming).
    pub async fn list_citing_paper(&self, paper_id: &str) -> Result<Vec<String>, crate::DbError> {
        self.list_id_column(queries::GRAPH_CITING_PAPER, paper_id)
            .await
    }

    async fn list_id_column(
        &self,
        sql: &str,
        paper_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(sql, [Value::Text(paper_id.to_string())]).await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(id) = crate::get_opt_text(&row, 0) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Record that `citing_paper_id` cites `cited_paper_id`. Idempotent; a
    /// self-citation (equal ids) is rejected.
    pub async fn insert_citation(
        &self,
        citing_paper_id: &str,
        cited_paper_id: &str,
    ) -> Result<(), crate::DbError> {
        if citing_paper_id == cited_paper_id {
            return Ok(());
        }
        let conn = self.conn();
        conn.execute(
            queries::CITATION_INSERT,
            [
                Value::Text(citing_paper_id.to_string()),
                Value::Text(cited_paper_id.to_string()),
            ],
        )
        .await?;
        self.touch(
            "paper_citations",
            crate::clock::Pk::Composite(citing_paper_id, cited_paper_id),
        )
        .await?;
        Ok(())
    }

    /// Read a one-time-task flag from `app_flags` (local-only bookkeeping).
    pub async fn get_app_flag(&self, key: &str) -> Result<Option<String>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(queries::APP_FLAG_SELECT, [Value::Text(key.to_string())])
            .await?;
        if let Some(row) = rows.next().await? {
            Ok(row.get_value(0)?.as_text().cloned())
        } else {
            Ok(None)
        }
    }

    /// Set a one-time-task flag in `app_flags`.
    pub async fn set_app_flag(&self, key: &str, value: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::APP_FLAG_UPSERT,
            [Value::Text(key.to_string()), Value::Text(value.to_string())],
        )
        .await?;
        Ok(())
    }
}

//! Concept pages: methods, datasets, benchmarks, tasks, and ideas.
//!
//! The slug is the identity. A tombstoned row with that slug is revived rather
//! than inserted again, because the sync key has to stay stable for edges that
//! already name it.

use chrono::{DateTime, Utc};
use rotero_models::{Concept, ConceptKind};
use turso::Value;

use crate::Database;
use crate::queries;

impl Database {
    /// Insert a concept or merge it into the live or tombstoned row with the
    /// same kind and slug.
    ///
    /// `body` of `None` leaves an existing body alone and stores an empty body
    /// on create. An empty title, or a title with no letters or digits, is
    /// rejected.
    pub async fn upsert_concept(
        &self,
        kind: ConceptKind,
        title: &str,
        body: Option<&str>,
    ) -> Result<Concept, crate::DbError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(crate::DbError::Rejected("concept title is empty".into()));
        }
        let slug = rotero_models::slug_key(title);
        if slug.is_empty() {
            return Err(crate::DbError::Rejected(
                "concept title has no letters or digits".into(),
            ));
        }

        if let Some((id, deleted)) = self.find_concept_by_slug(kind, &slug).await? {
            let existing = self.get_concept_row(&id).await?;
            let new_body = body.unwrap_or(existing.body.as_str()).to_string();
            if !deleted && existing.title == title && existing.body == new_body {
                return Ok(existing);
            }
            let now = Utc::now();
            self.conn()
                .execute(
                    queries::CONCEPT_UPDATE,
                    turso::params::Params::Positional(vec![
                        Value::Text(title.to_string()),
                        Value::Text(new_body),
                        Value::Text(now.to_rfc3339()),
                        Value::Text(id.clone()),
                    ]),
                )
                .await?;
            // `touch` clears `deleted`, which is how a tombstone comes back.
            self.touch("concepts", crate::clock::Pk::Single(&id))
                .await?;
            return self.get_concept_row(&id).await;
        }

        let now = Utc::now();
        let id = uuid::Uuid::now_v7().to_string();
        let stored_body = body.unwrap_or("").to_string();
        self.conn()
            .execute(
                queries::CONCEPT_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(id.clone()),
                    Value::Text(kind.as_str().to_string()),
                    Value::Text(title.to_string()),
                    Value::Text(slug),
                    Value::Text(stored_body),
                    Value::Text(now.to_rfc3339()),
                    Value::Text(now.to_rfc3339()),
                ]),
            )
            .await?;
        self.touch("concepts", crate::clock::Pk::Single(&id))
            .await?;
        self.get_concept_row(&id).await
    }

    /// Every live concept, optionally narrowed by kind and a case-insensitive
    /// substring of the title, slug, or body.
    pub async fn list_concepts(
        &self,
        kind: Option<ConceptKind>,
        query: Option<&str>,
    ) -> Result<Vec<Concept>, crate::DbError> {
        let sql = queries::CONCEPT_LIST.replace("{COLS}", queries::CONCEPT_COLS);
        let mut rows = self.conn().query(&sql, ()).await?;
        let all: Vec<Concept> = crate::collect_rows(&mut rows).await?;
        let needle = query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| q.to_lowercase());
        Ok(all
            .into_iter()
            .filter(|c| kind.is_none_or(|k| c.kind == k))
            .filter(|c| match &needle {
                None => true,
                Some(n) => {
                    c.title.to_lowercase().contains(n)
                        || c.slug.contains(n.as_str())
                        || c.body.to_lowercase().contains(n)
                }
            })
            .collect())
    }

    /// One live concept.
    pub async fn get_concept(&self, id: &str) -> Result<Option<Concept>, crate::DbError> {
        self.concept_by_sql(queries::CONCEPT_GET, id).await
    }

    /// Full-text search, falling back to LIKE when the index is missing.
    pub async fn search_concepts(&self, query: &str) -> Result<Vec<Concept>, crate::DbError> {
        let match_query = rotero_models::build_fts_match_query(query);
        if match_query.is_empty() {
            return Ok(Vec::new());
        }
        let sql = queries::CONCEPT_SEARCH_FTS.replace("{COLS}", queries::CONCEPT_COLS);
        match self.conn().query(&sql, [Value::Text(match_query)]).await {
            Ok(mut rows) => Ok(crate::collect_rows(&mut rows).await?),
            Err(_) => {
                let like = queries::CONCEPT_SEARCH_LIKE.replace("{COLS}", queries::CONCEPT_COLS);
                let pattern = format!("%{}%", query.trim());
                let mut rows = self.conn().query(&like, [Value::Text(pattern)]).await?;
                Ok(crate::collect_rows(&mut rows).await?)
            }
        }
    }

    async fn find_concept_by_slug(
        &self,
        kind: ConceptKind,
        slug: &str,
    ) -> Result<Option<(String, bool)>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::CONCEPT_FIND_BY_SLUG,
                [
                    Value::Text(kind.as_str().to_string()),
                    Value::Text(slug.to_string()),
                ],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let id = crate::get_text(&row, 0);
        let deleted = crate::get_bool(&row, 1);
        if id.is_empty() {
            return Ok(None);
        }
        Ok(Some((id, deleted)))
    }

    async fn get_concept_row(&self, id: &str) -> Result<Concept, crate::DbError> {
        self.concept_by_sql(queries::CONCEPT_GET_BASE, id)
            .await?
            .ok_or_else(|| crate::DbError::Rejected(format!("no concept {id}")))
    }

    async fn concept_by_sql(&self, sql: &str, id: &str) -> Result<Option<Concept>, crate::DbError> {
        let sql = sql.replace("{COLS}", queries::CONCEPT_COLS);
        let mut rows = self
            .conn()
            .query(&sql, [Value::Text(id.to_string())])
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        Ok(Some(concept_from_row(&row)))
    }
}

impl crate::FromRow for Concept {
    fn from_row(row: &turso::Row) -> Self {
        concept_from_row(row)
    }
}

pub(crate) fn concept_from_row(row: &turso::Row) -> Concept {
    let kind = crate::get_text(row, 1);
    Concept {
        id: crate::get_opt_text(row, 0),
        kind: ConceptKind::parse(&kind).unwrap_or(ConceptKind::Idea),
        title: crate::get_text(row, 2),
        slug: crate::get_text(row, 3),
        body: crate::get_text(row, 4),
        created_at: parse_time(&crate::get_text(row, 5)),
        modified_at: parse_time(&crate::get_text(row, 6)),
    }
}

pub(crate) fn parse_time(s: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

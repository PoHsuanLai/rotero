//! Claims: one sourced sentence per paper.
//!
//! Identity is `(paper_id, statement_key)`. A `from_chat` write never replaces
//! a claim that already quotes the paper. A quoted write may fill an empty
//! quote and may raise `from_chat` to `extracted`, and it never downgrades
//! `confirmed`.

use chrono::Utc;
use rotero_models::{Claim, ClaimDraft, ClaimStatus};
use turso::Value;

use crate::Database;
use crate::queries;

impl Database {
    /// File a claim, merging into the existing row for this paper and statement.
    pub async fn file_claim(&self, draft: &ClaimDraft) -> Result<Claim, crate::DbError> {
        let statement = rotero_models::normalize_statement(&draft.statement);
        if statement.is_empty() {
            return Err(crate::DbError::Rejected("claim statement is empty".into()));
        }
        let quote = draft.quote.trim().to_string();
        if draft.status.requires_quote() && quote.is_empty() {
            return Err(crate::DbError::Rejected(
                "a confirmed or extracted claim needs a quotation".into(),
            ));
        }
        if draft.page.is_some_and(|p| p < 1) {
            return Err(crate::DbError::Rejected(
                "claim page numbers are 1-based".into(),
            ));
        }
        let paper = self.get_paper_by_id(&draft.paper_id).await?;
        if paper.is_none() {
            return Err(crate::DbError::Rejected(format!(
                "no paper {}",
                draft.paper_id
            )));
        }
        let annotation_id = draft
            .annotation_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        if let Some((id, _deleted)) = self.find_claim(&draft.paper_id, &statement).await? {
            let existing = self.get_claim_row(&id).await?;
            let Some(merged) = merge_claim(&existing, &statement, &quote, draft, annotation_id)
            else {
                return Ok(existing);
            };
            self.write_claim(&id, &merged).await?;
            self.touch("claims", crate::clock::Pk::Single(&id)).await?;
            return self.get_claim_row(&id).await;
        }

        let now = Utc::now();
        let id = uuid::Uuid::now_v7().to_string();
        self.conn()
            .execute(
                queries::CLAIM_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(id.clone()),
                    Value::Text(draft.paper_id.clone()),
                    Value::Text(statement.clone()),
                    Value::Text(statement),
                    Value::Text(quote),
                    draft
                        .page
                        .map(|p| Value::Integer(p as i64))
                        .unwrap_or(Value::Null),
                    annotation_id.map(Value::Text).unwrap_or(Value::Null),
                    Value::Text(draft.status.as_str().to_string()),
                    Value::Text(now.to_rfc3339()),
                    Value::Text(now.to_rfc3339()),
                ]),
            )
            .await?;
        self.touch("claims", crate::clock::Pk::Single(&id)).await?;
        self.get_claim_row(&id).await
    }

    /// Live claims for one paper, oldest first.
    pub async fn list_claims_for_paper(
        &self,
        paper_id: &str,
    ) -> Result<Vec<Claim>, crate::DbError> {
        let sql = queries::CLAIM_LIST_FOR_PAPER.replace("{COLS}", queries::CLAIM_COLS);
        let mut rows = self
            .conn()
            .query(&sql, [Value::Text(paper_id.to_string())])
            .await?;
        Ok(crate::collect_rows(&mut rows).await?)
    }

    /// One live claim.
    pub async fn get_claim(&self, id: &str) -> Result<Option<Claim>, crate::DbError> {
        self.claim_by_sql(queries::CLAIM_GET, id).await
    }

    /// Full-text search. The caller sorts by status.
    pub async fn search_claims(&self, query: &str) -> Result<Vec<Claim>, crate::DbError> {
        let match_query = rotero_models::build_fts_match_query(query);
        if match_query.is_empty() {
            return Ok(Vec::new());
        }
        let sql = queries::CLAIM_SEARCH_FTS.replace("{COLS}", queries::CLAIM_COLS);
        let found = match self.conn().query(&sql, [Value::Text(match_query)]).await {
            Ok(mut rows) => crate::collect_rows(&mut rows).await?,
            Err(_) => {
                let like = queries::CLAIM_SEARCH_LIKE.replace("{COLS}", queries::CLAIM_COLS);
                let pattern = format!("%{}%", query.trim());
                let mut rows = self.conn().query(&like, [Value::Text(pattern)]).await?;
                crate::collect_rows(&mut rows).await?
            }
        };
        Ok(found)
    }

    async fn find_claim(
        &self,
        paper_id: &str,
        statement_key: &str,
    ) -> Result<Option<(String, bool)>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::CLAIM_FIND_BY_KEY,
                [
                    Value::Text(paper_id.to_string()),
                    Value::Text(statement_key.to_string()),
                ],
            )
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        let id = crate::get_text(&row, 0);
        if id.is_empty() {
            return Ok(None);
        }
        Ok(Some((id, crate::get_bool(&row, 1))))
    }

    async fn get_claim_row(&self, id: &str) -> Result<Claim, crate::DbError> {
        self.claim_by_sql(queries::CLAIM_GET_BASE, id)
            .await?
            .ok_or_else(|| crate::DbError::Rejected(format!("no claim {id}")))
    }

    async fn claim_by_sql(&self, sql: &str, id: &str) -> Result<Option<Claim>, crate::DbError> {
        let sql = sql.replace("{COLS}", queries::CLAIM_COLS);
        let mut rows = self
            .conn()
            .query(&sql, [Value::Text(id.to_string())])
            .await?;
        let Some(row) = rows.next().await? else {
            return Ok(None);
        };
        Ok(Some(claim_from_row(&row)))
    }

    async fn write_claim(&self, id: &str, claim: &Claim) -> Result<(), crate::DbError> {
        let key = rotero_models::normalize_statement(&claim.statement);
        self.conn()
            .execute(
                queries::CLAIM_UPDATE,
                turso::params::Params::Positional(vec![
                    Value::Text(claim.statement.clone()),
                    Value::Text(key),
                    Value::Text(claim.quote.clone()),
                    claim
                        .page
                        .map(|p| Value::Integer(p as i64))
                        .unwrap_or(Value::Null),
                    claim
                        .annotation_id
                        .clone()
                        .map(Value::Text)
                        .unwrap_or(Value::Null),
                    Value::Text(claim.status.as_str().to_string()),
                    Value::Text(Utc::now().to_rfc3339()),
                    Value::Text(id.to_string()),
                ]),
            )
            .await?;
        Ok(())
    }
}

/// The row to store, or `None` when the incoming draft must not change it.
fn merge_claim(
    existing: &Claim,
    statement: &str,
    quote: &str,
    draft: &ClaimDraft,
    annotation_id: Option<String>,
) -> Option<Claim> {
    // A chat answer never overwrites a claim that already quotes the paper,
    // and it does not refresh a previous chat answer either.
    if draft.status == ClaimStatus::FromChat {
        return None;
    }

    let mut next = existing.clone();
    next.statement = statement.to_string();
    if existing.quote.trim().is_empty() {
        next.quote = quote.to_string();
    }
    if existing.page.is_none() {
        next.page = draft.page;
    }
    if existing.annotation_id.is_none() {
        next.annotation_id = annotation_id;
    }
    next.status = match existing.status {
        ClaimStatus::Confirmed => ClaimStatus::Confirmed,
        ClaimStatus::FromChat => {
            if draft.status == ClaimStatus::Confirmed {
                ClaimStatus::Confirmed
            } else {
                ClaimStatus::Extracted
            }
        }
        ClaimStatus::Extracted => {
            if draft.status == ClaimStatus::Confirmed {
                ClaimStatus::Confirmed
            } else {
                ClaimStatus::Extracted
            }
        }
    };

    let changed = next.statement != existing.statement
        || next.quote != existing.quote
        || next.page != existing.page
        || next.annotation_id != existing.annotation_id
        || next.status != existing.status;
    changed.then_some(next)
}

impl crate::FromRow for Claim {
    fn from_row(row: &turso::Row) -> Self {
        claim_from_row(row)
    }
}

pub(crate) fn claim_from_row(row: &turso::Row) -> Claim {
    let status = crate::get_text(row, 6);
    Claim {
        id: crate::get_opt_text(row, 0),
        paper_id: crate::get_text(row, 1),
        statement: crate::get_text(row, 2),
        quote: crate::get_text(row, 3),
        page: crate::get_opt_i64(row, 4).map(|n| n as i32),
        annotation_id: crate::get_opt_text(row, 5).filter(|s| !s.is_empty()),
        status: ClaimStatus::parse(&status).unwrap_or(ClaimStatus::Extracted),
        created_at: crate::concepts::parse_time(&crate::get_text(row, 7)),
        modified_at: crate::concepts::parse_time(&crate::get_text(row, 8)),
    }
}

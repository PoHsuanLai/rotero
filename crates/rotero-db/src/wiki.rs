//! Edges between claims and concepts, unresolved citation stubs, and the
//! search that reads both plus the paper index.
//!
//! Edge identity is the five columns `(src_kind, src_id, rel, dst_kind, dst_id)`.
//! The row's sync key is a surrogate id, because a clock stamp addresses one
//! column or two. A tombstoned edge with the same five columns is revived.

use chrono::Utc;
use rotero_models::{
    CitationRecord, Claim, Concept, ConceptPage, ENDPOINT_CLAIM, ENDPOINT_CONCEPT, PaperId,
    ReferenceStub, WikiRel, WikiSearch,
};
use turso::Value;

use crate::Database;
use crate::claims::claim_from_row;
use crate::concepts::concept_from_row;
use crate::queries;

impl Database {
    /// Link a claim to the concept it is about. Idempotent.
    pub async fn link_claim_concept(
        &self,
        claim_id: &str,
        concept_id: &str,
    ) -> Result<String, crate::DbError> {
        self.require_live_claim(claim_id).await?;
        self.require_live_concept(concept_id).await?;
        self.upsert_edge(
            ENDPOINT_CLAIM,
            claim_id,
            WikiRel::About,
            ENDPOINT_CONCEPT,
            concept_id,
        )
        .await
    }

    /// Link two claims. `rel` is `supports`, `qualifies`, or `disputes`.
    /// A claim cannot link to itself.
    pub async fn link_claims(
        &self,
        src_claim_id: &str,
        dst_claim_id: &str,
        rel: WikiRel,
    ) -> Result<String, crate::DbError> {
        if !matches!(
            rel,
            WikiRel::Supports | WikiRel::Qualifies | WikiRel::Disputes
        ) {
            return Err(crate::DbError::Rejected(
                "link_claims only accepts supports, qualifies, or disputes".into(),
            ));
        }
        if src_claim_id == dst_claim_id {
            return Err(crate::DbError::Rejected(
                "a claim cannot relate to itself".into(),
            ));
        }
        self.require_live_claim(src_claim_id).await?;
        self.require_live_claim(dst_claim_id).await?;
        self.upsert_edge(
            ENDPOINT_CLAIM,
            src_claim_id,
            rel,
            ENDPOINT_CLAIM,
            dst_claim_id,
        )
        .await
    }

    /// Relate two concepts. Stored once, with the lesser id as the source.
    pub async fn link_concepts(&self, a: &str, b: &str) -> Result<String, crate::DbError> {
        if a == b {
            return Err(crate::DbError::Rejected(
                "a concept cannot relate to itself".into(),
            ));
        }
        self.require_live_concept(a).await?;
        self.require_live_concept(b).await?;
        let (src, dst) = if a < b { (a, b) } else { (b, a) };
        self.upsert_edge(
            ENDPOINT_CONCEPT,
            src,
            WikiRel::Related,
            ENDPOINT_CONCEPT,
            dst,
        )
        .await
    }

    /// The concept page, the claims about it, and the concepts related to it.
    pub async fn read_concept(&self, id: &str) -> Result<ConceptPage, crate::DbError> {
        let concept = self.require_live_concept(id).await?;
        let claims = self.claims_about(id).await?;
        let related = self.related_concepts(id).await?;
        Ok(ConceptPage {
            concept,
            claims,
            related,
        })
    }

    /// Concepts a claim is about, for the paper detail panel.
    pub async fn concepts_for_claim(&self, claim_id: &str) -> Result<Vec<Concept>, crate::DbError> {
        let sql = queries::WIKI_CONCEPTS_FOR_CLAIM;
        let mut rows = self
            .conn()
            .query(sql, [Value::Text(claim_id.to_string())])
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(concept_from_row(&row));
        }
        Ok(out)
    }

    /// Concepts, claims, and papers for one query.
    ///
    /// Claims come back confirmed, then extracted, then `from_chat`.
    pub async fn search_wiki(&self, query: &str) -> Result<WikiSearch, crate::DbError> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Err(crate::DbError::Rejected(
                "wiki search query is empty".into(),
            ));
        }
        let mut concepts = self.search_concepts(trimmed).await?;
        concepts.truncate(20);
        let mut claims = self.search_claims(trimmed).await?;
        claims.sort_by_key(|c| (c.status.rank(), c.statement.clone()));
        claims.truncate(20);
        let papers = self.search_papers(trimmed).await?;
        Ok(WikiSearch {
            concepts,
            claims,
            papers,
        })
    }

    /// Record one external PDF link as a citation or a reference stub.
    ///
    /// A URI that does not parse as a DOI, arXiv id, PMID, or ISBN, and that
    /// does not match a library paper, is ignored.
    pub async fn note_external_citation(
        &self,
        citing_id: &str,
        uri: &str,
    ) -> Result<CitationRecord, crate::DbError> {
        if let Some(paper) = self.find_paper_by_link(uri).await?
            && let Some(cited_id) = paper.id.clone()
        {
            if cited_id == citing_id {
                return Ok(CitationRecord::Ignored);
            }
            self.insert_citation(citing_id, &cited_id).await?;
            return Ok(CitationRecord::Citation { cited_id });
        }
        let Some(pid) = PaperId::from_url(uri).or_else(|| PaperId::parse(uri)) else {
            return Ok(CitationRecord::Ignored);
        };
        let id = self
            .upsert_stub(citing_id, &pid.to_stored_string(), uri)
            .await?;
        Ok(CitationRecord::Stub { id })
    }

    /// Hide stubs whose identifier is now a paper in the library.
    pub async fn retire_stubs_matching_paper(
        &self,
        paper: &rotero_models::Paper,
    ) -> Result<(), crate::DbError> {
        let Some(pid) = paper.paper_id() else {
            return Ok(());
        };
        for variant in pid.stored_string_variants() {
            self.retire_stubs_with_identifier(&variant).await?;
        }
        Ok(())
    }

    /// Live stubs, optionally only those cited by one paper.
    pub async fn list_stubs(
        &self,
        citing_paper_id: Option<&str>,
    ) -> Result<Vec<ReferenceStub>, crate::DbError> {
        let mut rows = match citing_paper_id {
            Some(id) => {
                self.conn()
                    .query(queries::STUB_LIST_FOR_PAPER, [Value::Text(id.to_string())])
                    .await?
            }
            None => self.conn().query(queries::STUB_LIST, ()).await?,
        };
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(stub_from_row(&row));
        }
        Ok(out)
    }

    /// Tombstone every edge that names this claim, on either end.
    pub async fn tombstone_edges_touching_claim(
        &self,
        claim_id: &str,
    ) -> Result<(), crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::WIKI_EDGE_IDS_TOUCHING_CLAIM,
                [Value::Text(claim_id.to_string())],
            )
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = crate::get_text(&row, 0);
            if !id.is_empty() {
                ids.push(id);
            }
        }
        for id in ids {
            self.tombstone("wiki_edges", crate::clock::Pk::Single(&id))
                .await?;
        }
        Ok(())
    }

    async fn retire_stubs_with_identifier(&self, identifier: &str) -> Result<(), crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::STUB_IDS_FOR_IDENTIFIER,
                [Value::Text(identifier.to_string())],
            )
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = crate::get_text(&row, 0);
            if !id.is_empty() {
                ids.push(id);
            }
        }
        for id in ids {
            self.tombstone("reference_stubs", crate::clock::Pk::Single(&id))
                .await?;
        }
        Ok(())
    }

    async fn upsert_stub(
        &self,
        citing_id: &str,
        identifier: &str,
        raw: &str,
    ) -> Result<String, crate::DbError> {
        if self.get_paper_by_id(citing_id).await?.is_none() {
            return Err(crate::DbError::Rejected(format!("no paper {citing_id}")));
        }
        if let Some((id, deleted)) = self.find_stub(citing_id, identifier).await? {
            if deleted {
                self.touch("reference_stubs", crate::clock::Pk::Single(&id))
                    .await?;
            }
            return Ok(id);
        }
        let id = uuid::Uuid::now_v7().to_string();
        self.conn()
            .execute(
                queries::STUB_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(id.clone()),
                    Value::Text(citing_id.to_string()),
                    Value::Text(identifier.to_string()),
                    Value::Text(raw.to_string()),
                    Value::Text(Utc::now().to_rfc3339()),
                ]),
            )
            .await?;
        self.touch("reference_stubs", crate::clock::Pk::Single(&id))
            .await?;
        Ok(id)
    }

    async fn find_stub(
        &self,
        citing_id: &str,
        identifier: &str,
    ) -> Result<Option<(String, bool)>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::STUB_FIND,
                [
                    Value::Text(citing_id.to_string()),
                    Value::Text(identifier.to_string()),
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

    async fn upsert_edge(
        &self,
        src_kind: &str,
        src_id: &str,
        rel: WikiRel,
        dst_kind: &str,
        dst_id: &str,
    ) -> Result<String, crate::DbError> {
        if let Some((id, deleted)) = self
            .find_edge(src_kind, src_id, rel, dst_kind, dst_id)
            .await?
        {
            if deleted {
                self.touch("wiki_edges", crate::clock::Pk::Single(&id))
                    .await?;
            }
            return Ok(id);
        }
        let id = uuid::Uuid::now_v7().to_string();
        self.conn()
            .execute(
                queries::WIKI_EDGE_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(id.clone()),
                    Value::Text(src_kind.to_string()),
                    Value::Text(src_id.to_string()),
                    Value::Text(rel.as_str().to_string()),
                    Value::Text(dst_kind.to_string()),
                    Value::Text(dst_id.to_string()),
                ]),
            )
            .await?;
        self.touch("wiki_edges", crate::clock::Pk::Single(&id))
            .await?;
        Ok(id)
    }

    async fn find_edge(
        &self,
        src_kind: &str,
        src_id: &str,
        rel: WikiRel,
        dst_kind: &str,
        dst_id: &str,
    ) -> Result<Option<(String, bool)>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::WIKI_EDGE_FIND,
                [
                    Value::Text(src_kind.to_string()),
                    Value::Text(src_id.to_string()),
                    Value::Text(rel.as_str().to_string()),
                    Value::Text(dst_kind.to_string()),
                    Value::Text(dst_id.to_string()),
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

    async fn require_live_claim(&self, id: &str) -> Result<Claim, crate::DbError> {
        self.get_claim(id)
            .await?
            .ok_or_else(|| crate::DbError::Rejected(format!("no claim {id}")))
    }

    async fn require_live_concept(&self, id: &str) -> Result<Concept, crate::DbError> {
        self.get_concept(id)
            .await?
            .ok_or_else(|| crate::DbError::Rejected(format!("no concept {id}")))
    }

    async fn claims_about(&self, concept_id: &str) -> Result<Vec<Claim>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::WIKI_CLAIMS_ABOUT_CONCEPT,
                [Value::Text(concept_id.to_string())],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(claim_from_row(&row));
        }
        Ok(out)
    }

    async fn related_concepts(&self, concept_id: &str) -> Result<Vec<Concept>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(
                queries::WIKI_RELATED_CONCEPTS,
                [Value::Text(concept_id.to_string())],
            )
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(concept_from_row(&row));
        }
        Ok(out)
    }
}

fn stub_from_row(row: &turso::Row) -> ReferenceStub {
    ReferenceStub {
        id: crate::get_text(row, 0),
        citing_paper_id: crate::get_text(row, 1),
        identifier: crate::get_text(row, 2),
        raw: crate::get_text(row, 3),
        created_at: crate::concepts::parse_time(&crate::get_text(row, 4)),
    }
}

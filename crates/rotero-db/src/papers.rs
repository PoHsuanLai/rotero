use chrono::Utc;
use rotero_models::{CitationInfo, Creator, LibraryStatus, Paper, PaperLinks, Publication};
use turso::Value;

use crate::Database;
use crate::crr::{PaperCollections, PaperTags, Papers};
use crate::queries;

/// The `extra_meta` JSON key under which the venue fields that have no dedicated
/// column (ISBN, ISSN, series, place, language) are nested, so they don't collide
/// with a translator-supplied `citation.extra_meta` payload sharing the blob.
const VENUE_META_KEY: &str = "__venue";

/// Serialize a paper's `citation.extra_meta` together with its column-less venue
/// fields into the single `extra_meta` JSON string stored in the DB. Returns
/// `None` when there is nothing to store. Public so the MCP crate's parallel
/// insert path encodes the `extra_meta` column identically.
pub fn encode_extra_meta(paper: &Paper) -> Option<String> {
    let mut root = match &paper.citation.extra_meta {
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(other) => {
            // A non-object extra_meta is preserved under a reserved key so the
            // venue payload can still be attached alongside it.
            let mut map = serde_json::Map::new();
            map.insert("__extra".to_string(), other.clone());
            map
        }
        None => serde_json::Map::new(),
    };

    let mut venue = serde_json::Map::new();
    let p = &paper.publication;
    for (k, v) in [
        ("isbn", &p.isbn),
        ("issn", &p.issn),
        ("series", &p.series),
        ("place", &p.place),
        ("language", &p.language),
    ] {
        if let Some(val) = v.as_deref().filter(|s| !s.is_empty()) {
            venue.insert(k.to_string(), serde_json::Value::String(val.to_string()));
        }
    }
    if !venue.is_empty() {
        root.insert(VENUE_META_KEY.to_string(), serde_json::Value::Object(venue));
    }

    if root.is_empty() {
        None
    } else {
        serde_json::to_string(&serde_json::Value::Object(root)).ok()
    }
}

/// Split a stored `extra_meta` JSON string back into the model's
/// `citation.extra_meta` (with the venue payload and any `__extra` wrapper
/// removed) and the venue fields it carried.
fn decode_extra_meta(raw: Option<&str>) -> (Option<serde_json::Value>, VenueFields) {
    let Some(value) = raw.and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) else {
        return (None, VenueFields::default());
    };
    let serde_json::Value::Object(mut map) = value else {
        return (Some(value), VenueFields::default());
    };

    let venue = match map.remove(VENUE_META_KEY) {
        Some(serde_json::Value::Object(v)) => {
            let get = |k: &str| {
                v.get(k)
                    .and_then(|x| x.as_str())
                    .map(str::to_string)
                    .filter(|s| !s.is_empty())
            };
            VenueFields {
                isbn: get("isbn"),
                issn: get("issn"),
                series: get("series"),
                place: get("place"),
                language: get("language"),
            }
        }
        _ => VenueFields::default(),
    };

    // Unwrap a non-object extra_meta that was preserved under `__extra`.
    let extra = match map.remove("__extra") {
        Some(inner) if map.is_empty() => Some(inner),
        Some(inner) => {
            map.insert("__extra".to_string(), inner);
            Some(serde_json::Value::Object(map))
        }
        None if map.is_empty() => None,
        None => Some(serde_json::Value::Object(map)),
    };

    (extra, venue)
}

/// The column-less venue fields recovered from `extra_meta`.
#[derive(Default)]
struct VenueFields {
    isbn: Option<String>,
    issn: Option<String>,
    series: Option<String>,
    place: Option<String>,
    language: Option<String>,
}

impl Database {
    /// Insert a new paper and return its generated UUID.
    pub async fn insert_paper(&self, paper: &Paper) -> Result<String, crate::DbError> {
        let conn = self.conn();
        let uuid = uuid::Uuid::now_v7().to_string();
        let authors_json =
            serde_json::to_string(&paper.creators).unwrap_or_else(|_| "[]".to_string());
        let extra_meta = encode_extra_meta(paper);

        use crate::{opt_int, opt_text};
        conn.execute(
            queries::PAPER_INSERT,
            turso::params::Params::Positional(vec![
                Value::Text(uuid.clone()),
                Value::Text(paper.title.clone()),
                Value::Text(authors_json),
                paper
                    .year
                    .map(|y| Value::Integer(y as i64))
                    .unwrap_or(Value::Null),
                opt_text(paper.canonical_doi().as_ref()),
                opt_text(paper.abstract_text.as_ref()),
                opt_text(paper.publication.journal.as_ref()),
                opt_text(paper.publication.volume.as_ref()),
                opt_text(paper.publication.issue.as_ref()),
                opt_text(paper.publication.pages.as_ref()),
                opt_text(paper.publication.publisher.as_ref()),
                opt_text(paper.links.url.as_ref()),
                opt_text(paper.links.pdf_path.as_ref()),
                Value::Text(paper.status.date_added.to_rfc3339()),
                Value::Text(paper.status.date_modified.to_rfc3339()),
                Value::Integer(paper.status.is_favorite as i64),
                Value::Integer(paper.status.is_read as i64),
                extra_meta.map(Value::Text).unwrap_or(Value::Null),
                opt_int(paper.citation.citation_count),
                opt_text(paper.citation.citation_key.as_ref()),
                opt_text(paper.links.pdf_url.as_ref()),
                Value::Text(paper.item_type.clone()),
            ]),
        )
        .await?;

        self.crr()
            .track_insert("papers", &uuid, Papers::ALL)
            .await?;
        self.touch("papers", crate::clock::Pk::Single(&uuid)).await?;

        Ok(uuid)
    }

    /// List all papers, returning up to 500 most recently added.
    pub async fn list_papers(&self) -> Result<Vec<Paper>, crate::DbError> {
        self.list_papers_paginated(0, 500).await
    }

    /// List papers ordered by date added, with offset/limit pagination.
    pub async fn list_papers_paginated(
        &self,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Paper>, crate::DbError> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {} FROM papers_live ORDER BY date_added DESC LIMIT ?1 OFFSET ?2",
            queries::PAPER_SELECT_COLS
        );
        let mut rows = conn
            .query(
                &sql,
                [Value::Integer(limit as i64), Value::Integer(offset as i64)],
            )
            .await?;
        crate::collect_rows(&mut rows).await.map_err(Into::into)
    }

    /// Return the total number of papers in the library.
    pub async fn count_papers(&self) -> Result<u32, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::PAPER_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    /// Search papers. If the query parses as an identifier (DOI, arXiv id, …), a
    /// direct lookup is tried first. Otherwise it runs BM25 full-text search and
    /// applies a light re-rank that guarantees exact/prefix title matches land at
    /// the very top; falls back to LIKE if FTS is unavailable.
    pub async fn search_papers(&self, query: &str) -> Result<Vec<Paper>, crate::DbError> {
        let conn = self.conn();
        let trimmed = query.trim();

        // Fast path: a bare identifier matches the stored `doi` string exactly.
        // Canonicalize first so `10.48550/arXiv.X` finds the row stored as `arXiv:X`.
        if let Some(pid) = rotero_models::PaperId::parse(trimmed) {
            let hits = search_papers_by_doi(conn, &pid.to_stored_string()).await?;
            if !hits.is_empty() {
                return Ok(hits);
            }
        }

        let candidates = match search_papers_fts(conn, query).await {
            Ok(results) => results,
            Err(_) => search_papers_like(conn, query).await?,
        };
        Ok(rotero_models::rank_local_results(candidates, query))
    }

    /// Resolve a link URL (as found inside a PDF) to a library paper.
    ///
    /// Tries, in order: the identifier the URL carries (DOI / arXiv / PMID,
    /// canonicalized to the stored form and matched exactly against `doi`), then
    /// an exact match of the raw URL against the stored `url` / `pdf_url`.
    /// Returns the first match, or `None` if nothing in the library matches.
    pub async fn find_paper_by_link(&self, url: &str) -> Result<Option<Paper>, crate::DbError> {
        let conn = self.conn();

        if let Some(pid) = rotero_models::PaperId::from_url(url) {
            // Try every stored form the identifier may take (arXiv papers are
            // stored either as `arXiv:X` or as their raw `10.48550/arXiv.X` DOI).
            for stored in pid.stored_string_variants() {
                let hits = search_papers_by_doi(conn, &stored).await?;
                if let Some(p) = hits.into_iter().next() {
                    return Ok(Some(p));
                }
            }
        }

        let trimmed = url.trim();
        if !trimmed.is_empty() {
            let hits = search_papers_by_url(conn, trimmed).await?;
            if let Some(p) = hits.into_iter().next() {
                return Ok(Some(p));
            }
        }

        Ok(None)
    }

    /// Fetch papers by a list of IDs.
    pub async fn get_papers_by_ids(&self, ids: &[String]) -> Result<Vec<Paper>, crate::DbError> {
        let conn = self.conn();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "SELECT {} FROM papers_live WHERE id IN ({})",
            queries::PAPER_SELECT_COLS,
            placeholders.join(", ")
        );
        let params: Vec<Value> = ids.iter().map(|id| Value::Text(id.clone())).collect();
        let mut rows = conn
            .query(&sql, turso::params::Params::Positional(params))
            .await?;
        crate::collect_rows(&mut rows).await.map_err(Into::into)
    }

    /// Toggle the favorite flag on a paper.
    pub async fn set_favorite(&self, id: &str, favorite: bool) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_SET_FAVORITE,
            [Value::Integer(favorite as i64), Value::Text(id.to_string())],
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::IS_FAVORITE])
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Toggle the read flag on a paper.
    pub async fn set_read(&self, id: &str, read: bool) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_SET_READ,
            [Value::Integer(read as i64), Value::Text(id.to_string())],
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::IS_READ])
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Store extracted full-text content for a paper (used for FTS indexing).
    /// Store extracted PDF text for a paper.
    ///
    /// Deliberately does not stamp the row's sync clock. `fulltext` is
    /// local-only — re-extractable from the PDF and excluded from every snapshot
    /// — so bumping the clock here would let a background extraction on one
    /// device outrank, and silently discard, a real metadata edit made on
    /// another.
    pub async fn update_paper_fulltext(&self, id: &str, text: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_UPDATE_FULLTEXT,
            turso::params::Params::Positional(vec![
                Value::Text(text.to_string()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        Ok(())
    }

    /// Update a paper's bibliographic metadata (title, authors, DOI, etc.).
    pub async fn update_paper_metadata(
        &self,
        id: &str,
        paper: &Paper,
    ) -> Result<(), crate::DbError> {
        let conn = self.conn();
        use crate::opt_text;
        let authors_json =
            serde_json::to_string(&paper.creators).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            queries::PAPER_UPDATE_METADATA,
            turso::params::Params::Positional(vec![
                Value::Text(paper.title.clone()),
                Value::Text(authors_json),
                paper
                    .year
                    .map(|y| Value::Integer(y as i64))
                    .unwrap_or(Value::Null),
                opt_text(paper.canonical_doi().as_ref()),
                opt_text(paper.abstract_text.as_ref()),
                opt_text(paper.publication.journal.as_ref()),
                opt_text(paper.publication.volume.as_ref()),
                opt_text(paper.publication.issue.as_ref()),
                opt_text(paper.publication.pages.as_ref()),
                opt_text(paper.publication.publisher.as_ref()),
                opt_text(paper.links.url.as_ref()),
                Value::Text(Utc::now().to_rfc3339()),
                Value::Text(paper.item_type.clone()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr()
            .track_update(
                "papers",
                id,
                &[
                    Papers::TITLE,
                    Papers::AUTHORS,
                    Papers::YEAR,
                    Papers::DOI,
                    Papers::ABSTRACT_TEXT,
                    Papers::JOURNAL,
                    Papers::VOLUME,
                    Papers::ISSUE,
                    Papers::PAGES,
                    Papers::PUBLISHER,
                    Papers::URL,
                    Papers::DATE_MODIFIED,
                    Papers::ITEM_TYPE,
                ],
            )
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Set a paper's title, leaving its other bibliographic fields untouched.
    ///
    /// Distinct from [`Database::update_paper_metadata`], which rewrites the
    /// whole bibliographic row: a user renaming one paper should not overwrite
    /// fields enrichment may have filled in since, nor mark them all dirty for
    /// sync. The FTS index covers `title` directly, so it follows this write.
    pub async fn update_paper_title(&self, id: &str, title: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_UPDATE_TITLE,
            turso::params::Params::Positional(vec![
                Value::Text(title.to_string()),
                Value::Text(Utc::now().to_rfc3339()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::TITLE, Papers::DATE_MODIFIED])
            .await?;
        Ok(())
    }

    /// Update the relative PDF file path for a paper.
    pub async fn update_pdf_path(&self, id: &str, pdf_path: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_UPDATE_PDF_PATH,
            turso::params::Params::Positional(vec![
                Value::Text(pdf_path.to_string()),
                Value::Text(chrono::Utc::now().to_rfc3339()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::PDF_PATH, Papers::DATE_MODIFIED])
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Update a paper's `date_modified` to now.
    pub async fn touch_paper(&self, id: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            queries::PAPER_TOUCH,
            [Value::Text(now), Value::Text(id.to_string())],
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::DATE_MODIFIED])
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Delete a paper by ID along with its annotations, notes, and memberships.
    ///
    /// The children are removed explicitly rather than by foreign key. The schema
    /// declares `ON DELETE CASCADE`, but `PRAGMA foreign_keys` is off, so nothing
    /// ever fired and every deleted paper left its rows behind — this comment used
    /// to claim a cascade that did not exist.
    ///
    /// Turning the pragma on would not be enough on its own: a cascade happens
    /// inside SQLite, so the child rows would vanish locally with no `track_delete`
    /// and peers would keep them forever, holding memberships that point at a
    /// paper that no longer exists. Deleting them here means each one is tracked
    /// and actually reaches the other devices.
    pub async fn delete_paper(&self, id: &str) -> Result<(), crate::DbError> {
        let collections = self.collection_ids_for_paper(id).await?;
        let tags = self.tag_ids_for_paper(id).await?;
        let annotations = self.child_ids("annotations", "paper_id", id).await?;
        let notes = self.child_ids("notes", "paper_id", id).await?;
        let citing = self.citation_pks(id, true).await?;
        let cited = self.citation_pks(id, false).await?;

        let conn = self.conn();

        // Tombstoned, not removed. A hard delete leaves nothing to publish, so
        // a peer still holding the child row would treat its copy as news and
        // resurrect it — the paper would come back one annotation at a time.
        let now = chrono::Utc::now().timestamp_millis();
        let device = self.device_id().to_string();

        for table in ["annotations", "notes", "paper_collections", "paper_tags"] {
            conn.execute(
                &format!(
                    "UPDATE {table} SET deleted = 1, updated_at = ?2, updated_by = ?3 \
                     WHERE paper_id = ?1"
                ),
                turso::params::Params::Positional(vec![
                    Value::Text(id.to_string()),
                    Value::Integer(now),
                    Value::Text(device.clone()),
                ]),
            )
            .await?;
        }
        conn.execute(
            "UPDATE paper_citations SET deleted = 1, updated_at = ?2, updated_by = ?3 \
             WHERE citing_paper_id = ?1 OR cited_paper_id = ?1",
            turso::params::Params::Positional(vec![
                Value::Text(id.to_string()),
                Value::Integer(now),
                Value::Text(device),
            ]),
        )
        .await?;

        self.crr().track_delete("papers", id).await?;
        self.tombstone("papers", crate::clock::Pk::Single(id)).await?;
        for annotation_id in &annotations {
            self.crr()
                .track_delete("annotations", annotation_id)
                .await?;
            self.tombstone("annotations", crate::clock::Pk::Single(annotation_id)).await?;
        }
        for note_id in &notes {
            self.crr().track_delete("notes", note_id).await?;
            self.tombstone("notes", crate::clock::Pk::Single(note_id)).await?;
        }
        for collection_id in &collections {
            self.crr()
                .track_delete("paper_collections", &format!("{id}:{collection_id}"))
                .await?;
            self.tombstone("paper_collections", crate::clock::Pk::Composite(id, collection_id)).await?;
        }
        for tag_id in &tags {
            self.crr()
                .track_delete("paper_tags", &format!("{id}:{tag_id}"))
                .await?;
            self.tombstone("paper_tags", crate::clock::Pk::Composite(id, tag_id)).await?;
        }
        for pk in citing.iter().chain(cited.iter()) {
            self.crr().track_delete("paper_citations", pk).await?;
            self.tombstone("paper_citations", crate::clock::Pk::Single(pk)).await?;
        }

        Ok(())
    }

    /// Primary keys of a paper's child rows in a table keyed by `column`.
    async fn child_ids(
        &self,
        table: &str,
        column: &str,
        paper_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        self.junction_ids(
            &format!("SELECT id FROM {table} WHERE {column} = ?1"),
            paper_id,
        )
        .await
    }

    /// Composite keys of a paper's citation edges, in whichever direction.
    async fn citation_pks(
        &self,
        paper_id: &str,
        outgoing: bool,
    ) -> Result<Vec<String>, crate::DbError> {
        let sql = if outgoing {
            "SELECT citing_paper_id || ':' || cited_paper_id FROM paper_citations \
             WHERE citing_paper_id = ?1"
        } else {
            "SELECT citing_paper_id || ':' || cited_paper_id FROM paper_citations \
             WHERE cited_paper_id = ?1"
        };
        self.junction_ids(sql, paper_id).await
    }

    /// Returns groups of 2+ papers that share the same DOI or normalized title.
    pub async fn find_duplicates(&self) -> Result<Vec<Vec<Paper>>, crate::DbError> {
        let conn = self.conn();
        let mut groups: Vec<Vec<Paper>> = Vec::new();

        // Exact DOI duplicates
        let doi_sql =
            queries::PAPER_FIND_DOI_DUPLICATES.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = conn.query(&doi_sql, ()).await?;
        let doi_papers: Vec<Paper> = crate::collect_rows(&mut rows).await?;
        let mut current_doi = String::new();
        let mut current_group: Vec<Paper> = Vec::new();
        for paper in doi_papers {
            let doi = paper.doi.as_deref().unwrap_or_default();
            if doi != current_doi.as_str() && !current_group.is_empty() {
                groups.push(std::mem::take(&mut current_group));
            }
            current_doi = doi.to_string();
            current_group.push(paper);
        }
        if !current_group.is_empty() {
            groups.push(current_group);
        }

        // Normalized title duplicates (excluding papers already found by DOI)
        let doi_ids: std::collections::HashSet<String> = groups
            .iter()
            .flatten()
            .filter_map(|p| p.id.clone())
            .collect();
        let all = self.list_papers().await?;
        let mut title_map: std::collections::HashMap<String, Vec<Paper>> =
            std::collections::HashMap::new();
        for paper in all {
            if paper.id.as_ref().is_some_and(|id| doi_ids.contains(id)) {
                continue;
            }
            let normalized = rotero_models::normalize_title(&paper.title);
            if normalized.is_empty() {
                continue;
            }
            title_map.entry(normalized).or_default().push(paper);
        }
        for papers in title_map.into_values() {
            if papers.len() > 1 {
                groups.push(papers);
            }
        }

        Ok(groups)
    }

    /// Transfer associations from `delete_id` to `keep_id`, then delete the duplicate.
    ///
    /// Each moved membership is tracked individually rather than left to the bulk
    /// `INSERT ... SELECT`. Those statements wrote junction rows with no clock
    /// entries at all, so a merge stayed local: the surviving paper kept its tags
    /// on the machine that did the merge and lost them everywhere else, with the
    /// other devices still holding memberships that pointed at a paper that no
    /// longer existed. Extra sync rounds could not repair it, because there was
    /// nothing in the clock to send.
    pub async fn merge_papers(&self, keep_id: &str, delete_id: &str) -> Result<(), crate::DbError> {
        // Read the duplicate's memberships before moving them: `INSERT OR IGNORE`
        // hides which rows it actually created, and a membership the survivor
        // already had must not be re-tracked.
        let collections = self.collection_ids_for_paper(delete_id).await?;
        let existing_collections = self.collection_ids_for_paper(keep_id).await?;
        let tags = self.tag_ids_for_paper(delete_id).await?;
        let existing_tags = self.tag_ids_for_paper(keep_id).await?;

        let conn = self.conn();
        conn.execute(
            queries::PAPER_MERGE_COLLECTIONS,
            [
                Value::Text(keep_id.to_string()),
                Value::Text(delete_id.to_string()),
            ],
        )
        .await?;
        conn.execute(
            queries::PAPER_MERGE_TAGS,
            [
                Value::Text(keep_id.to_string()),
                Value::Text(delete_id.to_string()),
            ],
        )
        .await?;

        for collection_id in collections
            .iter()
            .filter(|id| !existing_collections.contains(id))
        {
            let pk = format!("{keep_id}:{collection_id}");
            self.crr()
                .track_insert("paper_collections", &pk, PaperCollections::ALL)
                .await?;
            self.touch("paper_collections", crate::clock::Pk::Composite(keep_id, collection_id)).await?;
        }
        for tag_id in tags.iter().filter(|id| !existing_tags.contains(id)) {
            let pk = format!("{keep_id}:{tag_id}");
            self.crr()
                .track_insert("paper_tags", &pk, PaperTags::ALL)
                .await?;
            self.touch("paper_tags", crate::clock::Pk::Composite(keep_id, tag_id)).await?;
        }

        // `delete_paper` tracks the duplicate's own junction rows as deleted, so
        // peers drop the memberships that pointed at it.
        self.delete_paper(delete_id).await?;
        Ok(())
    }

    /// Collection ids a paper belongs to.
    async fn collection_ids_for_paper(
        &self,
        paper_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        self.junction_ids(
            "SELECT collection_id FROM paper_collections WHERE paper_id = ?1",
            paper_id,
        )
        .await
    }

    /// Tag ids attached to a paper.
    async fn tag_ids_for_paper(&self, paper_id: &str) -> Result<Vec<String>, crate::DbError> {
        self.junction_ids(
            "SELECT tag_id FROM paper_tags WHERE paper_id = ?1",
            paper_id,
        )
        .await
    }

    pub(crate) async fn junction_ids(
        &self,
        sql: &str,
        paper_id: &str,
    ) -> Result<Vec<String>, crate::DbError> {
        let mut rows = self
            .conn()
            .query(sql, [Value::Text(paper_id.to_string())])
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(id) = row.get_value(0)?.as_text() {
                ids.push(id.clone());
            }
        }
        Ok(ids)
    }

    /// Return (id, doi) pairs for papers that have a DOI but no citation count yet.
    pub async fn list_papers_needing_citations(
        &self,
    ) -> Result<Vec<(String, String)>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(queries::PAPER_LIST_NEEDING_CITATIONS, ())
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
            let doi = row.get_value(1)?.as_text().cloned().unwrap_or_default();
            if !doi.is_empty() {
                out.push((id, doi));
            }
        }
        Ok(out)
    }

    /// Set the citation count for a paper (fetched from CrossRef).
    pub async fn update_citation_count(&self, id: &str, count: i64) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_UPDATE_CITATION_COUNT,
            [Value::Integer(count), Value::Text(id.to_string())],
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::CITATION_COUNT])
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Set the BibTeX citation key for a paper.
    pub async fn update_citation_key(&self, id: &str, key: &str) -> Result<(), crate::DbError> {
        let conn = self.conn();
        conn.execute(
            queries::PAPER_UPDATE_CITATION_KEY,
            turso::params::Params::Positional(vec![
                Value::Text(key.to_string()),
                Value::Text(id.to_string()),
            ]),
        )
        .await?;
        self.crr()
            .track_update("papers", id, &[Papers::CITATION_KEY])
            .await?;
        self.touch("papers", crate::clock::Pk::Single(id)).await?;
        Ok(())
    }

    /// Return (id, title, authors, year) for papers missing a citation key.
    pub async fn list_papers_needing_citation_keys(
        &self,
    ) -> Result<Vec<(String, String, Vec<String>, Option<i32>)>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn
            .query(queries::PAPER_LIST_NEEDING_CITATION_KEYS, ())
            .await?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
            let title = row
                .get_value(1)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_default();
            let authors_str = row
                .get_value(2)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_else(|| "[]".to_string());
            // The `authors` column holds a `Vec<Creator>` (dual-shape: legacy
            // plain strings or role-tagged objects). Keep only author-role
            // display names for citation-key generation.
            let creators: Vec<Creator> = serde_json::from_str(&authors_str).unwrap_or_default();
            let authors: Vec<String> = creators
                .iter()
                .filter(|c| c.role.is_author())
                .map(|c| c.display_name())
                .collect();
            let year = row
                .get_value(3)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .map(|y| y as i32);
            out.push((id, title, authors, year));
        }
        Ok(out)
    }

    /// List all existing citation keys (for dedup when generating new ones).
    pub async fn list_citation_keys(&self) -> Result<Vec<String>, crate::DbError> {
        let conn = self.conn();
        let mut rows = conn.query(queries::PAPER_LIST_CITATION_KEYS, ()).await?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(key) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
                keys.push(key);
            }
        }
        Ok(keys)
    }
}

async fn search_papers_by_doi(
    conn: &turso::Connection,
    stored_id: &str,
) -> Result<Vec<Paper>, crate::DbError> {
    let sql = queries::PAPER_SEARCH_BY_DOI.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn
        .query(&sql, [Value::Text(stored_id.to_string())])
        .await?;
    crate::collect_rows(&mut rows).await.map_err(Into::into)
}

async fn search_papers_by_url(
    conn: &turso::Connection,
    url: &str,
) -> Result<Vec<Paper>, crate::DbError> {
    let sql = queries::PAPER_SEARCH_BY_URL.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn.query(&sql, [Value::Text(url.to_string())]).await?;
    crate::collect_rows(&mut rows).await.map_err(Into::into)
}

async fn search_papers_fts(
    conn: &turso::Connection,
    query: &str,
) -> Result<Vec<Paper>, crate::DbError> {
    // Require every query token (AND) rather than any (turso's bare-token OR
    // default), so common words like "a" don't match the whole library and let
    // BM25 surface an unrelated high-frequency document.
    let match_query = rotero_models::build_fts_match_query(query);
    if match_query.is_empty() {
        return Ok(Vec::new());
    }
    let sql = queries::PAPER_SEARCH_FTS.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn.query(&sql, [Value::Text(match_query)]).await?;
    crate::collect_rows(&mut rows).await.map_err(Into::into)
}

async fn search_papers_like(
    conn: &turso::Connection,
    query: &str,
) -> Result<Vec<Paper>, crate::DbError> {
    let pattern = format!("%{query}%");
    let sql = queries::PAPER_SEARCH_LIKE.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn.query(&sql, [Value::Text(pattern)]).await?;
    crate::collect_rows(&mut rows).await.map_err(Into::into)
}

impl crate::FromRow for Paper {
    fn from_row(row: &turso::Row) -> Self {
        use crate::{get_bool, get_opt_i64, get_opt_text, get_text};
        // The `authors` column is a `Vec<Creator>`; the dual-shape deserializer
        // reads both legacy `["Name"]` strings and role-tagged objects.
        let authors_str = get_text(row, 2);
        let creators: Vec<Creator> = serde_json::from_str(&authors_str).unwrap_or_default();

        let date_added_str = get_text(row, 13);
        let date_modified_str = get_text(row, 14);
        let extra_meta_str = get_opt_text(row, 17);
        // Split the stored blob into the model's `extra_meta` and the column-less
        // venue fields nested under `VENUE_META_KEY`.
        let (extra_meta, venue) = decode_extra_meta(extra_meta_str.as_deref());

        Paper {
            id: get_opt_text(row, 0),
            // `item_type` is appended last (index 21); default for legacy rows.
            item_type: get_opt_text(row, 21).unwrap_or_else(|| "journalArticle".to_string()),
            title: get_text(row, 1),
            creators,
            year: get_opt_i64(row, 3).map(|i| i as i32),
            doi: get_opt_text(row, 4),
            abstract_text: get_opt_text(row, 5),
            publication: Publication {
                journal: get_opt_text(row, 6),
                volume: get_opt_text(row, 7),
                issue: get_opt_text(row, 8),
                pages: get_opt_text(row, 9),
                publisher: get_opt_text(row, 10),
                isbn: venue.isbn,
                issn: venue.issn,
                series: venue.series,
                place: venue.place,
                language: venue.language,
            },
            links: PaperLinks {
                url: get_opt_text(row, 11),
                pdf_path: get_opt_text(row, 12),
                pdf_url: get_opt_text(row, 20),
            },
            status: LibraryStatus {
                date_added: chrono::DateTime::parse_from_rfc3339(&date_added_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                date_modified: chrono::DateTime::parse_from_rfc3339(&date_modified_str)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                is_favorite: get_bool(row, 15),
                is_read: get_bool(row, 16),
            },
            citation: CitationInfo {
                citation_count: get_opt_i64(row, 18),
                citation_key: get_opt_text(row, 19),
                extra_meta,
            },
            search_rank: None,
        }
    }
}

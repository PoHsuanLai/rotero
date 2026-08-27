use std::path::Path;
use std::sync::Arc;

use rotero_db::FromRow;
use rotero_models::queries;
use rotero_models::{Annotation, Collection, Note, Paper, Tag};
use turso::{Connection, Value};

/// Callback invoked after every write operation so the UI can refresh.
pub type OnChangeFn = Arc<dyn Fn() + Send + Sync>;

/// Handle to the Rotero SQLite database for MCP queries.
#[derive(Clone)]
pub struct Database {
    conn: Connection,
    data_dir: std::path::PathBuf,
    on_change: Option<OnChangeFn>,
    device_id: Arc<str>,
}

impl Database {
    /// Open the SQLite database at the given path.
    ///
    /// Delegates to [`rotero_db::Database::open`] so the standalone server runs
    /// the same schema and migrations as the app. Opening the connection
    /// directly here skipped both, so writes against a fresh path committed and
    /// then failed change tracking.
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let data_dir = db_path.parent().ok_or("Invalid db path")?.to_path_buf();
        let db = rotero_db::Database::open(data_dir).await?;
        Ok(Self::from_db(&db))
    }

    /// Wrap the app's already-initialized database for embedded use.
    ///
    /// Shares the caller's connection rather than building a parallel one, so
    /// there is exactly one initialized store per process.
    pub fn from_db(db: &rotero_db::Database) -> Self {
        Self {
            conn: db.conn().clone(),
            data_dir: db.data_dir().to_path_buf(),
            on_change: None,
            device_id: db.device_id_arc(),
        }
    }

    /// Set a callback that fires after every write operation.
    #[allow(dead_code)]
    pub fn set_on_change(&mut self, f: OnChangeFn) {
        self.on_change = Some(f);
    }

    /// Notify the UI that data has changed.
    fn notify(&self) {
        if let Some(ref f) = self.on_change {
            f();
        }
    }

    /// Return the application data directory (parent of the database file).
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    /// Return the directory where imported PDF files are stored.
    pub fn papers_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("papers")
    }

    /// Resolve a relative PDF path to an absolute path under the papers directory.
    pub fn resolve_pdf_path(&self, rel_path: &str) -> std::path::PathBuf {
        self.papers_dir().join(rel_path)
    }

    /// Search papers by query string. Identifiers are looked up directly;
    /// otherwise BM25 full-text search runs, re-ranked so exact/prefix title
    /// matches lead; falls back to LIKE if FTS is unavailable.
    pub async fn search_papers(&self, query: &str) -> Result<Vec<Paper>, turso::Error> {
        let trimmed = query.trim();
        if let Some(pid) = rotero_models::PaperId::parse(trimmed) {
            let hits = self.search_papers_by_doi(&pid.to_stored_string()).await?;
            if !hits.is_empty() {
                return Ok(hits);
            }
        }
        let candidates = match self.search_papers_fts(query).await {
            Ok(results) => results,
            Err(_) => self.search_papers_like(query).await?,
        };
        Ok(rotero_models::rank_local_results(candidates, query))
    }

    async fn search_papers_by_doi(&self, stored_id: &str) -> Result<Vec<Paper>, turso::Error> {
        let sql = queries::PAPER_SEARCH_BY_DOI.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self
            .conn
            .query(&sql, [Value::Text(stored_id.to_string())])
            .await?;
        let mut papers = Vec::new();
        while let Some(row) = rows.next().await? {
            papers.push(Paper::from_row(&row));
        }
        Ok(papers)
    }

    async fn search_papers_fts(&self, query: &str) -> Result<Vec<Paper>, turso::Error> {
        // AND-join query tokens so all terms must be present (turso defaults to
        // OR, which lets a common word match the whole library). Mirrors
        // rotero-db's search so both consumers rank identically.
        let match_query = rotero_models::build_fts_match_query(query);
        if match_query.is_empty() {
            return Ok(Vec::new());
        }
        let sql = queries::PAPER_SEARCH_FTS.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self.conn.query(&sql, [Value::Text(match_query)]).await?;
        let mut papers = Vec::new();
        while let Some(row) = rows.next().await? {
            papers.push(Paper::from_row(&row));
        }
        Ok(papers)
    }

    async fn search_papers_like(&self, query: &str) -> Result<Vec<Paper>, turso::Error> {
        let pattern = format!("%{query}%");
        let sql = queries::PAPER_SEARCH_LIKE.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self.conn.query(&sql, [Value::Text(pattern)]).await?;
        let mut papers = Vec::new();
        while let Some(row) = rows.next().await? {
            papers.push(Paper::from_row(&row));
        }
        Ok(papers)
    }

    /// Fetch a single paper by its unique ID.
    pub async fn get_paper_by_id(&self, id: &str) -> Result<Option<Paper>, turso::Error> {
        let sql = queries::PAPER_GET_BY_ID.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self.conn.query(&sql, [Value::Text(id.to_string())]).await?;
        match rows.next().await? {
            Some(row) => Ok(Some(Paper::from_row(&row))),
            None => Ok(None),
        }
    }

    /// List papers with pagination (offset and limit).
    pub async fn list_papers(&self, offset: u32, limit: u32) -> Result<Vec<Paper>, turso::Error> {
        let sql = queries::PAPER_LIST_PAGINATED.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self
            .conn
            .query(
                &sql,
                [Value::Integer(limit as i64), Value::Integer(offset as i64)],
            )
            .await?;
        let mut papers = Vec::new();
        while let Some(row) = rows.next().await? {
            papers.push(Paper::from_row(&row));
        }
        Ok(papers)
    }

    /// Return the total number of papers in the library.
    pub async fn count_papers(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::PAPER_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    /// Return the number of unread papers.
    pub async fn count_unread(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::PAPER_COUNT_UNREAD, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    /// Return the number of favorited papers.
    pub async fn count_favorites(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::PAPER_COUNT_FAVORITES, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    /// Set or clear the favorite flag on a paper.
    pub async fn set_favorite(&self, id: &str, favorite: bool) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .set_favorite(id, favorite)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Set or clear the read flag on a paper.
    pub async fn set_read(&self, id: &str, read: bool) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .set_read(id, read)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// List all annotations (highlights, underlines, etc.) for a paper.
    pub async fn list_annotations_for_paper(
        &self,
        paper_id: &str,
    ) -> Result<Vec<Annotation>, turso::Error> {
        let mut rows = self
            .conn
            .query(
                queries::ANNOTATION_LIST_FOR_PAPER,
                [Value::Text(paper_id.to_string())],
            )
            .await?;
        let mut anns = Vec::new();
        while let Some(row) = rows.next().await? {
            anns.push(Annotation::from_row(&row));
        }
        Ok(anns)
    }

    /// List all user notes attached to a paper.
    pub async fn list_notes_for_paper(&self, paper_id: &str) -> Result<Vec<Note>, turso::Error> {
        let mut rows = self
            .conn
            .query(
                queries::NOTE_LIST_FOR_PAPER,
                [Value::Text(paper_id.to_string())],
            )
            .await?;
        let mut notes = Vec::new();
        while let Some(row) = rows.next().await? {
            notes.push(Note::from_row(&row));
        }
        Ok(notes)
    }

    /// Create a new note for a paper and return the generated note ID.
    pub async fn insert_note(
        &self,
        paper_id: &str,
        title: &str,
        body: &str,
    ) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .insert_note(&Note {
                body: body.to_string(),
                ..Note::new(paper_id.to_string(), title.to_string())
            })
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Update the title and body of an existing note.
    pub async fn update_note(&self, id: &str, title: &str, body: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .update_note(id, title, body)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// List all collections in the library.
    pub async fn list_collections(&self) -> Result<Vec<Collection>, turso::Error> {
        let mut rows = self.conn.query(queries::COLLECTION_LIST, ()).await?;
        let mut colls = Vec::new();
        while let Some(row) = rows.next().await? {
            colls.push(Collection {
                id: get_opt_text(&row, 0),
                name: row
                    .get_value(1)
                    .ok()
                    .and_then(|v| v.as_text().cloned())
                    .unwrap_or_default(),
                parent_id: get_opt_text(&row, 2),
                position: row
                    .get_value(3)
                    .ok()
                    .and_then(|v| v.as_integer().copied())
                    .unwrap_or(0) as i32,
            });
        }
        Ok(colls)
    }

    /// Return the total number of collections.
    pub async fn count_collections(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::COLLECTION_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    /// List paper IDs belonging to a specific collection.
    pub async fn list_paper_ids_in_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, turso::Error> {
        let mut rows = self
            .conn
            .query(
                queries::COLLECTION_PAPER_IDS,
                [Value::Text(collection_id.to_string())],
            )
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(id) = get_opt_text(&row, 0) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// List all tags in the library.
    pub async fn list_tags(&self) -> Result<Vec<Tag>, turso::Error> {
        let mut rows = self.conn.query(queries::TAG_LIST, ()).await?;
        let mut tags = Vec::new();
        while let Some(row) = rows.next().await? {
            tags.push(Tag {
                id: get_opt_text(&row, 0),
                name: row
                    .get_value(1)
                    .ok()
                    .and_then(|v| v.as_text().cloned())
                    .unwrap_or_default(),
                color: row.get_value(2).ok().and_then(|v| v.as_text().cloned()),
            });
        }
        Ok(tags)
    }

    /// Return the total number of tags.
    pub async fn count_tags(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::TAG_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    /// List paper IDs that have a specific tag.
    pub async fn list_paper_ids_by_tag(&self, tag_id: &str) -> Result<Vec<String>, turso::Error> {
        let mut rows = self
            .conn
            .query(queries::TAG_PAPER_IDS, [Value::Text(tag_id.to_string())])
            .await?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(id) = get_opt_text(&row, 0) {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// Find an existing tag by name, or create one with the given color.
    pub async fn get_or_create_tag(
        &self,
        name: &str,
        color: Option<&str>,
    ) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .get_or_create_tag(name, color)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Associate a tag with a paper.
    pub async fn add_tag_to_paper(&self, paper_id: &str, tag_id: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .add_tag_to_paper(paper_id, tag_id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Retrieve the extracted full text of a paper's PDF, if available.
    pub async fn get_paper_fulltext(&self, paper_id: &str) -> Result<Option<String>, turso::Error> {
        let mut rows = self
            .conn
            .query(
                queries::PAPER_SELECT_FULLTEXT,
                [Value::Text(paper_id.to_string())],
            )
            .await?;
        match rows.next().await? {
            Some(row) => Ok(get_opt_text(&row, 0)),
            None => Ok(None),
        }
    }

    /// Return all (paper_id, tag_id) pairs for building the relationship graph.
    pub async fn list_all_paper_tags(&self) -> Result<Vec<(String, String)>, turso::Error> {
        let mut rows = self
            .conn
            .query(queries::GRAPH_ALL_PAPER_TAGS, ())
            .await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            if let (Some(pid), Some(tid)) = (get_opt_text(&row, 0), get_opt_text(&row, 1)) {
                pairs.push((pid, tid));
            }
        }
        Ok(pairs)
    }

    /// Return all (paper_id, collection_id) pairs for building the relationship graph.
    pub async fn list_all_paper_collections(&self) -> Result<Vec<(String, String)>, turso::Error> {
        let mut rows = self
            .conn
            .query(queries::GRAPH_ALL_PAPER_COLLECTIONS, ())
            .await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            if let (Some(pid), Some(cid)) = (get_opt_text(&row, 0), get_opt_text(&row, 1)) {
                pairs.push((pid, cid));
            }
        }
        Ok(pairs)
    }

    /// List all directed (citing, cited) citation edges.
    pub async fn list_all_citations(&self) -> Result<Vec<(String, String)>, turso::Error> {
        let mut rows = self
            .conn
            .query(
                queries::GRAPH_ALL_CITATIONS,
                (),
            )
            .await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            if let (Some(a), Some(b)) = (get_opt_text(&row, 0), get_opt_text(&row, 1)) {
                pairs.push((a, b));
            }
        }
        Ok(pairs)
    }

    /// List all papers in the library (up to 10,000).
    pub async fn list_all_papers(&self) -> Result<Vec<Paper>, turso::Error> {
        let sql = queries::PAPER_LIST_PAGINATED.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self
            .conn
            .query(&sql, [Value::Integer(10000), Value::Integer(0)])
            .await?;
        let mut papers = Vec::new();
        while let Some(row) = rows.next().await? {
            papers.push(Paper::from_row(&row));
        }
        Ok(papers)
    }

    /// Insert a new paper and return its generated UUID.
    pub async fn insert_paper(&self, paper: &Paper) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .insert_paper(paper)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Update a paper's metadata fields. Only non-None fields are applied.
    pub async fn update_paper_metadata(&self, id: &str, paper: &Paper) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .update_paper_metadata(id, paper)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Delete a paper by ID along with its annotations, notes, and memberships.
    ///
    /// Delegates to `rotero_db` rather than issuing the delete here. The schema's
    /// `ON DELETE CASCADE` never fires (foreign keys are off), so the children
    /// have to be removed and tracked explicitly — and keeping one copy of that
    /// means the agent's deletes cannot drift from the app's.
    pub async fn delete_paper(&self, id: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .delete_paper(id)
            .await
            .map_err(|e| turso::Error::Error(e.to_string()))?;
        self.notify();
        Ok(())
    }

    /// View this handle as a `rotero_db::Database` sharing the same connection
    /// and device identity, so write paths can be reused instead of reimplemented.
    ///
    /// Every mutating method delegates through here. The agent and the app then
    /// run the same code, so a write path cannot be correct in one and wrong in
    /// the other — which is what happened when each kept its own copy.
    fn as_rotero_db(&self) -> rotero_db::Database {
        rotero_db::Database::from_parts(
            self.conn.clone(),
            self.data_dir.clone(),
            self.device_id.clone(),
        )
    }

    /// Remove a tag from a paper.
    pub async fn remove_tag_from_paper(
        &self,
        paper_id: &str,
        tag_id: &str,
    ) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .remove_tag_from_paper(paper_id, tag_id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Create a new collection and return its UUID.
    pub async fn insert_collection(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .insert_collection(&Collection {
                parent_id: parent_id.map(str::to_string),
                ..Collection::new(name.to_string())
            })
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Add a paper to a collection (idempotent).
    pub async fn add_paper_to_collection(
        &self,
        paper_id: &str,
        collection_id: &str,
    ) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .add_paper_to_collection(paper_id, collection_id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Remove a paper from a collection.
    pub async fn remove_paper_from_collection(
        &self,
        paper_id: &str,
        collection_id: &str,
    ) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .remove_paper_from_collection(paper_id, collection_id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Delete a collection (cascades to paper memberships).
    pub async fn delete_collection(&self, id: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .delete_collection(id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Rename a collection.
    pub async fn rename_collection(&self, id: &str, name: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .rename_collection(id, name)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Rename a tag.
    pub async fn rename_tag(&self, id: &str, name: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .rename_tag(id, name)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Delete a tag (cascades to paper-tag associations).
    pub async fn delete_tag(&self, id: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .delete_tag(id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Update the pdf_path for a paper after downloading a PDF.
    pub async fn update_pdf_path(&self, id: &str, pdf_path: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .update_pdf_path(id, pdf_path)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Delete a note by ID.
    pub async fn delete_note(&self, id: &str) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .delete_note(id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(())
    }
}

fn get_opt_text(row: &turso::Row, idx: usize) -> Option<String> {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned())
}

/// Map a `rotero_db` error into the `turso::Error` the MCP surface returns.
fn to_turso(e: rotero_db::DbError) -> turso::Error {
    turso::Error::Error(e.to_string())
}

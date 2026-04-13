use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
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
}

impl Database {
    /// Open the SQLite database at the given path.
    pub async fn open(db_path: &Path) -> Result<Self, String> {
        let data_dir = db_path.parent().ok_or("Invalid db path")?.to_path_buf();

        let db_path_str = db_path.to_string_lossy().to_string();

        let db = turso::Builder::new_local(&db_path_str)
            .build()
            .await
            .map_err(|e| format!("Failed to open database: {e}"))?;

        let conn = db
            .connect()
            .map_err(|e| format!("Failed to connect: {e}"))?;

        Ok(Self {
            conn,
            data_dir,
            on_change: None,
        })
    }

    /// Create from an existing connection (for embedding in the main app).
    pub fn from_conn(conn: Connection, data_dir: std::path::PathBuf) -> Self {
        Self {
            conn,
            data_dir,
            on_change: None,
        }
    }

    /// Set a callback that fires after every write operation.
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

    /// Search papers by query string, trying FTS first and falling back to LIKE.
    pub async fn search_papers(&self, query: &str) -> Result<Vec<Paper>, turso::Error> {
        match self.search_papers_fts(query).await {
            Ok(results) => Ok(results),
            Err(_) => self.search_papers_like(query).await,
        }
    }

    async fn search_papers_fts(&self, query: &str) -> Result<Vec<Paper>, turso::Error> {
        let sql = queries::PAPER_SEARCH_FTS.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self
            .conn
            .query(&sql, [Value::Text(query.to_string())])
            .await?;
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
        self.conn
            .execute(
                queries::PAPER_SET_FAVORITE,
                [Value::Integer(favorite as i64), Value::Text(id.to_string())],
            )
            .await?;
        self.notify();
        Ok(())
    }

    /// Set or clear the read flag on a paper.
    pub async fn set_read(&self, id: &str, read: bool) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::PAPER_SET_READ,
                [Value::Integer(read as i64), Value::Text(id.to_string())],
            )
            .await?;
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
        let now = Utc::now().to_rfc3339();
        let uuid = uuid::Uuid::now_v7().to_string();
        self.conn
            .execute(
                queries::NOTE_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(uuid.clone()),
                    Value::Text(paper_id.to_string()),
                    Value::Text(title.to_string()),
                    Value::Text(body.to_string()),
                    Value::Text(now.clone()),
                    Value::Text(now),
                ]),
            )
            .await?;
        self.notify();
        Ok(uuid)
    }

    /// Update the title and body of an existing note.
    pub async fn update_note(&self, id: &str, title: &str, body: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::NOTE_UPDATE,
                turso::params::Params::Positional(vec![
                    Value::Text(title.to_string()),
                    Value::Text(body.to_string()),
                    Value::Text(Utc::now().to_rfc3339()),
                    Value::Text(id.to_string()),
                ]),
            )
            .await?;
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
        let mut rows = self
            .conn
            .query(queries::TAG_FIND_BY_NAME, [Value::Text(name.to_string())])
            .await?;
        if let Some(row) = rows.next().await? {
            let id = get_opt_text(&row, 0).unwrap_or_default();
            return Ok(id);
        }
        let uuid = uuid::Uuid::now_v7().to_string();
        let actual_color = color.map(|c| c.to_string()).unwrap_or_else(|| {
            const PALETTE: &[&str] = &[
                "#6b7085", "#7c6b85", "#6b8580", "#857a6b", "#6b7a85", "#856b7a", "#6b856e",
                "#85706b", "#6e6b85", "#7a856b", "#856b6b", "#6b8585",
            ];
            let hash = name
                .bytes()
                .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
            PALETTE[hash % PALETTE.len()].to_string()
        });
        self.conn
            .execute(
                queries::TAG_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(uuid.clone()),
                    Value::Text(name.to_string()),
                    Value::Text(actual_color),
                ]),
            )
            .await?;
        self.notify();
        Ok(uuid)
    }

    /// Associate a tag with a paper.
    pub async fn add_tag_to_paper(&self, paper_id: &str, tag_id: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::TAG_ADD_TO_PAPER,
                [
                    Value::Text(paper_id.to_string()),
                    Value::Text(tag_id.to_string()),
                ],
            )
            .await?;
        self.notify();
        Ok(())
    }

    /// Retrieve the extracted full text of a paper's PDF, if available.
    pub async fn get_paper_fulltext(&self, paper_id: &str) -> Result<Option<String>, turso::Error> {
        let mut rows = self
            .conn
            .query(
                "SELECT fulltext FROM papers WHERE id = ?1",
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
            .query("SELECT paper_id, tag_id FROM paper_tags", ())
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
            .query("SELECT paper_id, collection_id FROM paper_collections", ())
            .await?;
        let mut pairs = Vec::new();
        while let Some(row) = rows.next().await? {
            if let (Some(pid), Some(cid)) = (get_opt_text(&row, 0), get_opt_text(&row, 1)) {
                pairs.push((pid, cid));
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
        let uuid = uuid::Uuid::now_v7().to_string();
        let authors_json =
            serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
        let extra_meta = paper
            .citation
            .extra_meta
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        self.conn
            .execute(
                queries::PAPER_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(uuid.clone()),
                    Value::Text(paper.title.clone()),
                    Value::Text(authors_json),
                    paper
                        .year
                        .map(|y| Value::Integer(y as i64))
                        .unwrap_or(Value::Null),
                    opt_text(paper.doi.as_ref()),
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
                    paper
                        .citation
                        .citation_count
                        .map(Value::Integer)
                        .unwrap_or(Value::Null),
                    opt_text(paper.citation.citation_key.as_ref()),
                    opt_text(paper.links.pdf_url.as_ref()),
                ]),
            )
            .await?;

        rotero_db::crr::tracking::track_insert(
            &self.conn,
            "papers",
            &uuid,
            &[
                "title",
                "authors",
                "year",
                "doi",
                "abstract_text",
                "journal",
                "volume",
                "issue",
                "pages",
                "publisher",
                "url",
                "pdf_path",
                "date_added",
                "date_modified",
                "is_favorite",
                "is_read",
                "extra_meta",
                "citation_count",
                "citation_key",
                "pdf_url",
            ],
        )
        .await?;

        self.notify();
        Ok(uuid)
    }

    /// Update a paper's metadata fields. Only non-None fields are applied.
    pub async fn update_paper_metadata(&self, id: &str, paper: &Paper) -> Result<(), turso::Error> {
        let authors_json =
            serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
        self.conn
            .execute(
                queries::PAPER_UPDATE_METADATA,
                turso::params::Params::Positional(vec![
                    Value::Text(paper.title.clone()),
                    Value::Text(authors_json),
                    paper
                        .year
                        .map(|y| Value::Integer(y as i64))
                        .unwrap_or(Value::Null),
                    opt_text(paper.doi.as_ref()),
                    opt_text(paper.abstract_text.as_ref()),
                    opt_text(paper.publication.journal.as_ref()),
                    opt_text(paper.publication.volume.as_ref()),
                    opt_text(paper.publication.issue.as_ref()),
                    opt_text(paper.publication.pages.as_ref()),
                    opt_text(paper.publication.publisher.as_ref()),
                    opt_text(paper.links.url.as_ref()),
                    Value::Text(Utc::now().to_rfc3339()),
                    Value::Text(id.to_string()),
                ]),
            )
            .await?;

        rotero_db::crr::tracking::track_update(
            &self.conn,
            "papers",
            id,
            &[
                "title",
                "authors",
                "year",
                "doi",
                "abstract_text",
                "journal",
                "volume",
                "issue",
                "pages",
                "publisher",
                "url",
                "date_modified",
            ],
        )
        .await?;

        self.notify();
        Ok(())
    }

    /// Delete a paper by ID (cascades to annotations, notes, memberships).
    pub async fn delete_paper(&self, id: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(queries::PAPER_DELETE, [Value::Text(id.to_string())])
            .await?;
        rotero_db::crr::tracking::track_delete(&self.conn, "papers", id).await?;
        self.notify();
        Ok(())
    }

    /// Remove a tag from a paper.
    pub async fn remove_tag_from_paper(
        &self,
        paper_id: &str,
        tag_id: &str,
    ) -> Result<(), turso::Error> {
        self.conn
            .execute(
                "DELETE FROM paper_tags WHERE paper_id = ?1 AND tag_id = ?2",
                [
                    Value::Text(paper_id.to_string()),
                    Value::Text(tag_id.to_string()),
                ],
            )
            .await?;
        let pk = format!("{paper_id}:{tag_id}");
        rotero_db::crr::tracking::track_delete(&self.conn, "paper_tags", &pk).await?;
        self.notify();
        Ok(())
    }

    /// Create a new collection and return its UUID.
    pub async fn insert_collection(
        &self,
        name: &str,
        parent_id: Option<&str>,
    ) -> Result<String, turso::Error> {
        let uuid = uuid::Uuid::now_v7().to_string();
        self.conn
            .execute(
                queries::COLLECTION_INSERT,
                turso::params::Params::Positional(vec![
                    Value::Text(uuid.clone()),
                    Value::Text(name.to_string()),
                    parent_id
                        .map(|s| Value::Text(s.to_string()))
                        .unwrap_or(Value::Null),
                    Value::Integer(0),
                ]),
            )
            .await?;
        rotero_db::crr::tracking::track_insert(
            &self.conn,
            "collections",
            &uuid,
            &["name", "parent_id", "position"],
        )
        .await?;
        self.notify();
        Ok(uuid)
    }

    /// Add a paper to a collection (idempotent).
    pub async fn add_paper_to_collection(
        &self,
        paper_id: &str,
        collection_id: &str,
    ) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::COLLECTION_ADD_PAPER,
                [
                    Value::Text(paper_id.to_string()),
                    Value::Text(collection_id.to_string()),
                ],
            )
            .await?;
        let pk = format!("{paper_id}:{collection_id}");
        rotero_db::crr::tracking::track_insert(
            &self.conn,
            "paper_collections",
            &pk,
            &["paper_id", "collection_id"],
        )
        .await?;
        self.notify();
        Ok(())
    }

    /// Remove a paper from a collection.
    pub async fn remove_paper_from_collection(
        &self,
        paper_id: &str,
        collection_id: &str,
    ) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::COLLECTION_REMOVE_PAPER,
                [
                    Value::Text(paper_id.to_string()),
                    Value::Text(collection_id.to_string()),
                ],
            )
            .await?;
        let pk = format!("{paper_id}:{collection_id}");
        rotero_db::crr::tracking::track_delete(&self.conn, "paper_collections", &pk).await?;
        self.notify();
        Ok(())
    }

    /// Delete a collection (cascades to paper memberships).
    pub async fn delete_collection(&self, id: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(queries::COLLECTION_DELETE, [Value::Text(id.to_string())])
            .await?;
        rotero_db::crr::tracking::track_delete(&self.conn, "collections", id).await?;
        self.notify();
        Ok(())
    }

    /// Rename a collection.
    pub async fn rename_collection(&self, id: &str, name: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::COLLECTION_RENAME,
                turso::params::Params::Positional(vec![
                    Value::Text(name.to_string()),
                    Value::Text(id.to_string()),
                ]),
            )
            .await?;
        rotero_db::crr::tracking::track_update(&self.conn, "collections", id, &["name"]).await?;
        self.notify();
        Ok(())
    }

    /// Rename a tag.
    pub async fn rename_tag(&self, id: &str, name: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::TAG_RENAME,
                turso::params::Params::Positional(vec![
                    Value::Text(name.to_string()),
                    Value::Text(id.to_string()),
                ]),
            )
            .await?;
        rotero_db::crr::tracking::track_update(&self.conn, "tags", id, &["name"]).await?;
        self.notify();
        Ok(())
    }

    /// Delete a tag (cascades to paper-tag associations).
    pub async fn delete_tag(&self, id: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(queries::TAG_DELETE, [Value::Text(id.to_string())])
            .await?;
        rotero_db::crr::tracking::track_delete(&self.conn, "tags", id).await?;
        self.notify();
        Ok(())
    }

    /// Delete a note by ID.
    pub async fn delete_note(&self, id: &str) -> Result<(), turso::Error> {
        self.conn
            .execute(queries::NOTE_DELETE, [Value::Text(id.to_string())])
            .await?;
        rotero_db::crr::tracking::track_delete(&self.conn, "notes", id).await?;
        self.notify();
        Ok(())
    }
}

fn get_opt_text(row: &turso::Row, idx: usize) -> Option<String> {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned())
}

fn opt_text(s: Option<&String>) -> Value {
    s.map(|v| Value::Text(v.clone())).unwrap_or(Value::Null)
}

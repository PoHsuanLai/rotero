use std::path::Path;

use chrono::Utc;
use rotero_db::FromRow;
use rotero_models::queries;
use rotero_models::{Annotation, Collection, Note, Paper, Tag};
use turso::{Connection, Value};

#[derive(Clone)]
pub struct Database {
    conn: Connection,
    data_dir: std::path::PathBuf,
}

impl Database {
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

        Ok(Self { conn, data_dir })
    }

    /// Create from an existing connection (for embedding in the main app).
    #[allow(dead_code)] // public API for embedding
    pub fn from_conn(conn: Connection, data_dir: std::path::PathBuf) -> Self {
        Self { conn, data_dir }
    }

    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    pub fn papers_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("papers")
    }

    pub fn resolve_pdf_path(&self, rel_path: &str) -> std::path::PathBuf {
        self.papers_dir().join(rel_path)
    }

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

    pub async fn get_paper_by_id(&self, id: &str) -> Result<Option<Paper>, turso::Error> {
        let sql = queries::PAPER_GET_BY_ID.replace("{COLS}", queries::PAPER_SELECT_COLS);
        let mut rows = self.conn.query(&sql, [Value::Text(id.to_string())]).await?;
        match rows.next().await? {
            Some(row) => Ok(Some(Paper::from_row(&row))),
            None => Ok(None),
        }
    }

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

    pub async fn count_papers(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::PAPER_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    pub async fn count_unread(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::PAPER_COUNT_UNREAD, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    pub async fn count_favorites(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::PAPER_COUNT_FAVORITES, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

    pub async fn set_favorite(&self, id: &str, favorite: bool) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::PAPER_SET_FAVORITE,
                [Value::Integer(favorite as i64), Value::Text(id.to_string())],
            )
            .await?;
        Ok(())
    }

    pub async fn set_read(&self, id: &str, read: bool) -> Result<(), turso::Error> {
        self.conn
            .execute(
                queries::PAPER_SET_READ,
                [Value::Integer(read as i64), Value::Text(id.to_string())],
            )
            .await?;
        Ok(())
    }

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
        Ok(uuid)
    }

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
        Ok(())
    }

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

    pub async fn count_collections(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::COLLECTION_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

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

    pub async fn count_tags(&self) -> Result<u32, turso::Error> {
        let mut rows = self.conn.query(queries::TAG_COUNT, ()).await?;
        let row = rows
            .next()
            .await?
            .ok_or(turso::Error::QueryReturnedNoRows)?;
        Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
    }

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
        Ok(uuid)
    }

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
        Ok(())
    }

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
}

fn get_opt_text(row: &turso::Row, idx: usize) -> Option<String> {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned())
}

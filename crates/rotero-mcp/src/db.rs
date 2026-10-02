use std::path::Path;
use std::sync::Arc;

use rotero_models::{Annotation, Collection, Note, Paper, Tag};
use turso::Connection;

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
    ///
    /// Called from the app crate; unused when this crate is built as a binary.
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

    /// Resolve a relative PDF path to an absolute path under the papers directory.
    pub fn resolve_pdf_path(&self, rel_path: &str) -> std::path::PathBuf {
        self.as_rotero_db().resolve_pdf_path(rel_path)
    }

    /// Search papers by query string. Identifiers are looked up directly;
    /// otherwise BM25 full-text search runs, re-ranked so exact/prefix title
    /// matches lead; falls back to LIKE if FTS is unavailable.
    pub async fn search_papers(&self, query: &str) -> Result<Vec<Paper>, turso::Error> {
        self.as_rotero_db()
            .search_papers(query)
            .await
            .map_err(to_turso)
    }

    /// Fetch a single paper by its unique ID.
    pub async fn get_paper_by_id(&self, id: &str) -> Result<Option<Paper>, turso::Error> {
        self.as_rotero_db()
            .get_paper_by_id(id)
            .await
            .map_err(to_turso)
    }

    /// List papers with pagination (offset and limit).
    pub async fn list_papers(&self, offset: u32, limit: u32) -> Result<Vec<Paper>, turso::Error> {
        self.as_rotero_db()
            .list_papers_paginated(offset, limit)
            .await
            .map_err(to_turso)
    }

    /// Return the total number of papers in the library.
    pub async fn count_papers(&self) -> Result<u32, turso::Error> {
        self.as_rotero_db().count_papers().await.map_err(to_turso)
    }

    /// Return the number of unread papers.
    pub async fn count_unread(&self) -> Result<u32, turso::Error> {
        self.as_rotero_db().count_unread().await.map_err(to_turso)
    }

    /// Return the number of favorited papers.
    pub async fn count_favorites(&self) -> Result<u32, turso::Error> {
        self.as_rotero_db()
            .count_favorites()
            .await
            .map_err(to_turso)
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

    /// Insert a highlight/note annotation into the library DB.
    pub async fn insert_annotation(&self, ann: &Annotation) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .insert_annotation(ann)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// List all annotations (highlights, underlines, etc.) for a paper.
    pub async fn list_annotations_for_paper(
        &self,
        paper_id: &str,
    ) -> Result<Vec<Annotation>, turso::Error> {
        self.as_rotero_db()
            .list_annotations_for_paper(paper_id)
            .await
            .map_err(to_turso)
    }

    /// List all user notes attached to a paper.
    pub async fn list_notes_for_paper(&self, paper_id: &str) -> Result<Vec<Note>, turso::Error> {
        self.as_rotero_db()
            .list_notes_for_paper(paper_id)
            .await
            .map_err(to_turso)
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
        self.as_rotero_db()
            .list_collections()
            .await
            .map_err(to_turso)
    }

    /// Return the total number of collections.
    pub async fn count_collections(&self) -> Result<u32, turso::Error> {
        self.as_rotero_db()
            .count_collections()
            .await
            .map_err(to_turso)
    }

    /// List paper IDs belonging to a specific collection.
    pub async fn list_paper_ids_in_collection(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, turso::Error> {
        self.as_rotero_db()
            .list_paper_ids_in_collection(collection_id)
            .await
            .map_err(to_turso)
    }

    /// List all tags in the library.
    pub async fn list_tags(&self) -> Result<Vec<Tag>, turso::Error> {
        self.as_rotero_db().list_tags().await.map_err(to_turso)
    }

    /// Return the total number of tags.
    pub async fn count_tags(&self) -> Result<u32, turso::Error> {
        self.as_rotero_db().count_tags().await.map_err(to_turso)
    }

    /// List paper IDs that have a specific tag.
    pub async fn list_paper_ids_by_tag(&self, tag_id: &str) -> Result<Vec<String>, turso::Error> {
        self.as_rotero_db()
            .list_paper_ids_by_tag(tag_id)
            .await
            .map_err(to_turso)
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
        self.as_rotero_db()
            .get_paper_fulltext(paper_id)
            .await
            .map_err(to_turso)
    }

    /// Return all (paper_id, tag_id) pairs for building the relationship graph.
    pub async fn list_all_paper_tags(&self) -> Result<Vec<(String, String)>, turso::Error> {
        self.as_rotero_db()
            .list_all_paper_tags()
            .await
            .map_err(to_turso)
    }

    /// Return all (paper_id, collection_id) pairs for building the relationship graph.
    pub async fn list_all_paper_collections(&self) -> Result<Vec<(String, String)>, turso::Error> {
        self.as_rotero_db()
            .list_all_paper_collections()
            .await
            .map_err(to_turso)
    }

    /// List all directed (citing, cited) citation edges.
    pub async fn list_all_citations(&self) -> Result<Vec<(String, String)>, turso::Error> {
        self.as_rotero_db()
            .list_all_citations()
            .await
            .map_err(to_turso)
    }

    /// Papers that `paper_id` cites (outgoing).
    pub async fn list_cited_by_paper(&self, paper_id: &str) -> Result<Vec<String>, turso::Error> {
        self.as_rotero_db()
            .list_cited_by_paper(paper_id)
            .await
            .map_err(to_turso)
    }

    /// Papers that cite `paper_id` (incoming).
    pub async fn list_citing_paper(&self, paper_id: &str) -> Result<Vec<String>, turso::Error> {
        self.as_rotero_db()
            .list_citing_paper(paper_id)
            .await
            .map_err(to_turso)
    }

    /// List all papers in the library (up to 10,000).
    pub async fn list_all_papers(&self) -> Result<Vec<Paper>, turso::Error> {
        self.as_rotero_db()
            .list_papers_paginated(0, 10_000)
            .await
            .map_err(to_turso)
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
        self.as_rotero_db().delete_tag(id).await.map_err(to_turso)?;
        self.notify();
        Ok(())
    }

    /// Update the pdf_path for a paper after downloading a PDF.
    ///
    /// `pdf_sha256` is the hash of the file now at that path, which the sync
    /// engine addresses the shared copy by. Callers that wrote the file through
    /// `import_pdf`/`import_pdf_bytes` already have it back from those.
    pub async fn update_pdf_path(
        &self,
        id: &str,
        pdf_path: &str,
        pdf_sha256: Option<&str>,
    ) -> Result<(), turso::Error> {
        self.as_rotero_db()
            .update_pdf_path(id, pdf_path, pdf_sha256)
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

    /// Live concepts, optionally narrowed by kind and a substring.
    pub async fn list_concepts(
        &self,
        kind: Option<rotero_models::ConceptKind>,
        query: Option<&str>,
    ) -> Result<Vec<rotero_models::Concept>, turso::Error> {
        self.as_rotero_db()
            .list_concepts(kind, query)
            .await
            .map_err(to_turso)
    }

    /// One concept page with its claims and related concepts.
    pub async fn read_concept(&self, id: &str) -> Result<rotero_models::ConceptPage, turso::Error> {
        self.as_rotero_db().read_concept(id).await.map_err(to_turso)
    }

    /// Insert or merge a concept page.
    pub async fn upsert_concept(
        &self,
        kind: rotero_models::ConceptKind,
        title: &str,
        body: Option<&str>,
    ) -> Result<rotero_models::Concept, turso::Error> {
        let concept = self
            .as_rotero_db()
            .upsert_concept(kind, title, body)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(concept)
    }

    /// Live claims for one paper.
    pub async fn list_claims_for_paper(
        &self,
        paper_id: &str,
    ) -> Result<Vec<rotero_models::Claim>, turso::Error> {
        self.as_rotero_db()
            .list_claims_for_paper(paper_id)
            .await
            .map_err(to_turso)
    }

    /// File a claim. Does not write notes, annotations, or paper metadata.
    pub async fn file_claim(
        &self,
        draft: &rotero_models::ClaimDraft,
    ) -> Result<rotero_models::Claim, turso::Error> {
        let claim = self
            .as_rotero_db()
            .file_claim(draft)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(claim)
    }

    /// Concepts, claims, and papers for one query.
    pub async fn search_wiki(
        &self,
        query: &str,
    ) -> Result<rotero_models::WikiSearch, turso::Error> {
        self.as_rotero_db()
            .search_wiki(query)
            .await
            .map_err(to_turso)
    }

    /// Link a claim to a concept.
    pub async fn link_claim_concept(
        &self,
        claim_id: &str,
        concept_id: &str,
    ) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .link_claim_concept(claim_id, concept_id)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Link two claims with supports, qualifies, or disputes.
    pub async fn link_claims(
        &self,
        src_claim_id: &str,
        dst_claim_id: &str,
        rel: rotero_models::WikiRel,
    ) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .link_claims(src_claim_id, dst_claim_id, rel)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Relate two concepts.
    pub async fn link_concepts(&self, a: &str, b: &str) -> Result<String, turso::Error> {
        let id = self
            .as_rotero_db()
            .link_concepts(a, b)
            .await
            .map_err(to_turso)?;
        self.notify();
        Ok(id)
    }

    /// Unresolved citation targets, optionally for one citing paper.
    pub async fn list_stubs(
        &self,
        citing_paper_id: Option<&str>,
    ) -> Result<Vec<rotero_models::ReferenceStub>, turso::Error> {
        self.as_rotero_db()
            .list_stubs(citing_paper_id)
            .await
            .map_err(to_turso)
    }

    /// Import PDF bytes using the library naming scheme.
    pub fn import_pdf_bytes(
        &self,
        bytes: &[u8],
        title: &str,
        first_author: Option<&str>,
        year: Option<i32>,
    ) -> Result<(String, String), String> {
        self.as_rotero_db()
            .import_pdf_bytes(bytes, title, first_author, year)
    }
}

/// Map a `rotero_db` error into the `turso::Error` the MCP surface returns.
fn to_turso(e: rotero_db::DbError) -> turso::Error {
    turso::Error::Error(e.to_string())
}

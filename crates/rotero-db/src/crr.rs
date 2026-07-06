//! Rotero's CRR (Conflict-free Replicated Relations) configuration.
//!
//! The CRDT engine itself lives in the external [`recrr`] crate; this module
//! only describes *which* tables and columns Rotero syncs and how to materialize
//! a skeleton row for each. The [`Database`](crate::Database) owns a
//! [`Crr`]`<`[`TursoDb`]`>` built from [`rotero_schema`].
//!
//! The schema is single-sourced from `#[derive(Crdt)]` structs below, so a
//! renamed or mistyped tracked column is a compile error rather than a
//! silent sync-drop. The generated column constants (e.g. [`Papers::TITLE`])
//! feed the `track_*` calls in `papers.rs` and the sibling table modules.

// The `#[derive(Crdt)]` structs below are schema *declarations*: their fields
// exist only to generate each table's `TableSpec` and column constants, and are
// never read as struct fields. The derive emits the constants we actually use.
#![allow(dead_code)]

use recrr::backends::TursoDb;
use recrr::{Crdt, Crr, Schema};

/// Rotero's change-tracking store: the CRR engine bound to the turso backend.
pub type CrrStore = Crr<TursoDb>;

/// Re-export the sync wire type so the sync transports don't depend on `recrr` directly.
pub use recrr::ChangeRow;

/// The `papers` table's synced columns.
///
/// `fulltext` is deliberately excluded — it is derived from the PDF and
/// re-extractable, so there's no need to sync it. Column *order* here does not
/// affect the schema fingerprint (recrr sorts columns before hashing), so this
/// mirrors the DB column order purely for readability.
#[derive(Crdt)]
#[crdt(table = "papers")]
pub struct Papers {
    #[crdt(pk)]
    id: String,
    #[crdt(skeleton = "")]
    title: String,
    #[crdt(skeleton = "[]")]
    authors: String,
    year: i64,
    doi: String,
    abstract_text: String,
    journal: String,
    volume: String,
    issue: String,
    pages: String,
    publisher: String,
    url: String,
    pdf_path: String,
    #[crdt(skeleton = now_rfc3339)]
    date_added: String,
    #[crdt(skeleton = now_rfc3339)]
    date_modified: String,
    #[crdt(skeleton = 0)]
    is_favorite: i64,
    #[crdt(skeleton = 0)]
    is_read: i64,
    extra_meta: String,
    citation_count: i64,
    citation_key: String,
    pdf_url: String,
    #[crdt(skeleton = "journalArticle")]
    item_type: String,
}

#[derive(Crdt)]
#[crdt(table = "collections")]
pub struct Collections {
    #[crdt(pk)]
    id: String,
    #[crdt(skeleton = "")]
    name: String,
    parent_id: String,
    #[crdt(skeleton = 0)]
    position: i64,
}

#[derive(Crdt)]
#[crdt(table = "tags")]
pub struct Tags {
    #[crdt(pk)]
    id: String,
    #[crdt(skeleton = "")]
    name: String,
    color: String,
}

#[derive(Crdt)]
#[crdt(table = "annotations")]
pub struct Annotations {
    #[crdt(pk)]
    id: String,
    #[crdt(skeleton = "")]
    paper_id: String,
    #[crdt(skeleton = 0)]
    page: i64,
    #[crdt(skeleton = "note")]
    ann_type: String,
    #[crdt(skeleton = "#ffff00")]
    color: String,
    content: String,
    #[crdt(skeleton = "{}")]
    geometry: String,
    #[crdt(skeleton = now_rfc3339)]
    created_at: String,
    #[crdt(skeleton = now_rfc3339)]
    modified_at: String,
}

#[derive(Crdt)]
#[crdt(table = "notes")]
pub struct Notes {
    #[crdt(pk)]
    id: String,
    #[crdt(skeleton = "")]
    paper_id: String,
    #[crdt(skeleton = "")]
    title: String,
    #[crdt(skeleton = "")]
    body: String,
    #[crdt(skeleton = now_rfc3339)]
    created_at: String,
    #[crdt(skeleton = now_rfc3339)]
    modified_at: String,
}

#[derive(Crdt)]
#[crdt(table = "saved_searches")]
pub struct SavedSearches {
    #[crdt(pk)]
    id: String,
    #[crdt(skeleton = "")]
    name: String,
    #[crdt(skeleton = "")]
    query: String,
    #[crdt(skeleton = now_rfc3339)]
    created_at: String,
}

#[derive(Crdt)]
#[crdt(table = "paper_collections", pk = (paper_id, collection_id; sep = ':'))]
pub struct PaperCollections {
    paper_id: String,
    collection_id: String,
}

#[derive(Crdt)]
#[crdt(table = "paper_tags", pk = (paper_id, tag_id; sep = ':'))]
pub struct PaperTags {
    paper_id: String,
    tag_id: String,
}

/// The set of tables and columns Rotero replicates, mirroring the app schema.
pub fn rotero_schema() -> Schema {
    recrr::schema![
        Papers,
        Collections,
        Tags,
        Annotations,
        Notes,
        SavedSearches,
        PaperCollections,
        PaperTags,
    ]
}

/// Build a CRR store for a turso connection using Rotero's schema.
///
/// Convenience for crates that hold a bare `turso::Connection` (e.g. the MCP
/// server) and don't depend on `recrr` directly. Call [`CrrStore::init`] once
/// after the app tables exist to create the CRR metadata tables.
pub fn make_crr_store(conn: turso::Connection) -> CrrStore {
    CrrStore::new(TursoDb::new(conn), rotero_schema())
}

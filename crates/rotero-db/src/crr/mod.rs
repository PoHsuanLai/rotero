//! CRR (Conflict-free Replicated Relations) change tracking and merge logic.
//!
//! Implements cr-sqlite-style CRDT semantics:
//! - Per-column LWW (Last-Writer-Wins) via version counters
//! - Causal length (CL) for delete/resurrect tracking
//! - Site ID for deterministic tie-breaking

mod helpers;
pub mod merge;
pub mod schema;
pub mod state;
pub mod tracking;

use serde::{Deserialize, Serialize};

/// A single column-level change record for sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRow {
    pub table_name: String,
    pub pk: String,
    pub col_name: String, // column name or "__sentinel" for row existence
    pub col_val: serde_json::Value,
    pub col_ver: i64,
    pub db_ver: i64,
    pub site_id: Vec<u8>, // 16-byte UUID
    pub seq: i64,
    pub cl: i64, // causal length (odd=alive, even=deleted)
}

/// Result of applying a changeset.
#[derive(Debug, Default)]
pub struct MergeResult {
    pub applied: usize,
    pub skipped: usize,
}

// ── Tables that participate in CRR ──────────────────────────────

/// All CRR-enabled tables and their non-PK columns.
const CRR_TABLES: &[(&str, &[&str])] = &[
    (
        "papers",
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
            // "fulltext" excluded — derived from PDF, re-extractable
            "citation_count",
            "citation_key",
        ],
    ),
    ("collections", &["name", "parent_id", "position"]),
    ("tags", &["name", "color"]),
    (
        "annotations",
        &[
            "paper_id",
            "page",
            "ann_type",
            "color",
            "content",
            "geometry",
            "created_at",
            "modified_at",
        ],
    ),
    (
        "notes",
        &["paper_id", "title", "body", "created_at", "modified_at"],
    ),
    ("saved_searches", &["name", "query", "created_at"]),
    ("paper_collections", &["paper_id", "collection_id"]),
    ("paper_tags", &["paper_id", "tag_id"]),
];

// ── Re-exports ──────────────────────────────────────────────────

pub use merge::apply_changes;
pub use schema::init_crr_tables;
pub use state::{current_db_version, get_sync_state, next_db_version, set_sync_state, site_id};
pub use tracking::{changes_since, track_delete, track_insert, track_update};

//! Rotero's CRR (Conflict-free Replicated Relations) configuration.
//!
//! The CRDT engine itself lives in the external [`recrr`] crate; this module
//! only describes *which* tables and columns Rotero syncs and how to materialize
//! a skeleton row for each. The [`Database`](crate::Database) owns a
//! [`Crr`]`<`[`TursoDb`]`>` built from [`rotero_schema`].

use recrr::backends::TursoDb;
use recrr::{Crr, PkSpec, Schema, SkeletonValue, TableSpec, Value};

/// Rotero's change-tracking store: the CRR engine bound to the turso backend.
pub type CrrStore = Crr<TursoDb>;

/// Re-export the sync wire type so the sync transports don't depend on `recrr` directly.
pub use recrr::ChangeRow;

/// Build a CRR store for a turso connection using Rotero's schema.
///
/// Convenience for crates that hold a bare `turso::Connection` (e.g. the MCP
/// server) and don't depend on `recrr` directly. Call [`CrrStore::init`] once
/// after the app tables exist to create the CRR metadata tables.
pub fn make_crr_store(conn: turso::Connection) -> CrrStore {
    CrrStore::new(TursoDb::new(conn), rotero_schema())
}

/// A NOT NULL text default of the empty string.
fn empty_text() -> SkeletonValue {
    SkeletonValue::Literal(Value::Text(String::new()))
}

/// A NOT NULL integer default.
fn int(n: i64) -> SkeletonValue {
    SkeletonValue::Literal(Value::Integer(n))
}

/// A NOT NULL text default of a fixed string literal.
fn text(s: &str) -> SkeletonValue {
    SkeletonValue::Literal(Value::Text(s.to_string()))
}

/// The set of tables and columns Rotero replicates, mirroring the app schema.
///
/// `fulltext` is deliberately excluded from `papers` — it is derived from the
/// PDF and re-extractable, so there's no need to sync it.
pub fn rotero_schema() -> Schema {
    Schema::new(vec![
        TableSpec::new(
            "papers",
            [
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
        .with_skeleton([
            ("title", empty_text()),
            ("authors", text("[]")),
            ("date_added", SkeletonValue::NowRfc3339),
            ("date_modified", SkeletonValue::NowRfc3339),
            ("is_favorite", int(0)),
            ("is_read", int(0)),
        ]),
        TableSpec::new("collections", ["name", "parent_id", "position"])
            .with_skeleton([("name", empty_text()), ("position", int(0))]),
        TableSpec::new("tags", ["name", "color"]).with_skeleton([("name", empty_text())]),
        TableSpec::new(
            "annotations",
            [
                "paper_id",
                "page",
                "ann_type",
                "color",
                "content",
                "geometry",
                "created_at",
                "modified_at",
            ],
        )
        .with_skeleton([
            ("paper_id", empty_text()),
            ("page", int(0)),
            ("ann_type", text("note")),
            ("color", text("#ffff00")),
            ("geometry", text("{}")),
            ("created_at", SkeletonValue::NowRfc3339),
            ("modified_at", SkeletonValue::NowRfc3339),
        ]),
        TableSpec::new(
            "notes",
            ["paper_id", "title", "body", "created_at", "modified_at"],
        )
        .with_skeleton([
            ("paper_id", empty_text()),
            ("title", empty_text()),
            ("body", empty_text()),
            ("created_at", SkeletonValue::NowRfc3339),
            ("modified_at", SkeletonValue::NowRfc3339),
        ]),
        TableSpec::new("saved_searches", ["name", "query", "created_at"]).with_skeleton([
            ("name", empty_text()),
            ("query", empty_text()),
            ("created_at", SkeletonValue::NowRfc3339),
        ]),
        TableSpec::new(
            "documents",
            [
                "title",
                "body",
                "collection_id",
                "template",
                "csl_style",
                "kind",
                "last_pdf_path",
                "created_at",
                "modified_at",
                "format",
            ],
        )
        .with_skeleton([
            ("title", empty_text()),
            ("body", empty_text()),
            ("template", text("article")),
            ("csl_style", text("apa")),
            ("kind", text("summary")),
            ("created_at", SkeletonValue::NowRfc3339),
            ("modified_at", SkeletonValue::NowRfc3339),
            ("format", text("typst")),
        ]),
        TableSpec::new("paper_collections", []).with_pk(PkSpec::composite(
            "paper_id",
            "collection_id",
            ':',
        )),
        TableSpec::new("paper_tags", []).with_pk(PkSpec::composite("paper_id", "tag_id", ':')),
    ])
}

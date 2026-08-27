//! CREATE TABLE statements for all core tables.
//!
//! The DDL itself lives in `sql/*.sql` so an editor highlights it as SQL;
//! `include_str!` pulls each file in at compile time, so these stay ordinary
//! `&'static str` constants with no runtime loading and no missing-file mode.

/// SQL batch that creates all core tables if they do not already exist.
pub const CREATE_TABLES: &str = include_str!("sql/tables.sql");

/// Views exposing only live (non-tombstoned) rows of each synced table.
///
/// Reads go through `<table>_live` so a query that forgets `deleted = 0` names a
/// relation that does not exist, rather than silently returning deleted papers.
/// Writes still target the base tables, and so do snapshot reads — a snapshot
/// has to carry tombstones or a deletion would never reach a peer.
///
/// `papers_live` deliberately has no FTS index: `idx_papers_fts` is built on
/// `papers` and cannot be filtered, so full-text search matches tombstoned rows
/// and drops them after the match.
pub const CREATE_LIVE_VIEWS: &str = include_str!("sql/live_views.sql");

/// SQL statement that creates the turso FTS index over paper text fields with weighted columns.
pub const CREATE_FTS_INDEX: &str = include_str!("sql/fts_index.sql");

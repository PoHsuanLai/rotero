//! SQL the sync engine generates per synced table.
//!
//! The statements in [`rotero_models::queries`] name one fixed table each. These
//! cannot: they are the same operation applied to all nine synced tables, with
//! the table and its key columns coming from [`SYNCED_TABLES`]. Building them
//! here keeps every generated statement in one file rather than scattered
//! through the modules that run them, and keeps the string-formatting off the
//! call sites.
//!
//! [`SYNCED_TABLES`]: crate::sync_schema::SYNCED_TABLES

use crate::sync_schema::{PkSpec, SYNC_COLUMNS, SyncedTable};

/// Stamp a row's sync clock, optionally tombstoning it.
///
/// `updated_at` is clamped to at least one millisecond past its current value:
/// two writes in one millisecond would otherwise tie, and a clock that jumps
/// backwards would let a device's own newer edit lose to its own older one.
pub fn stamp_row(table: &str, predicate: &str) -> String {
    format!(
        "UPDATE {table} SET \
            updated_at = MAX(?1, updated_at + 1), \
            updated_by = ?2, \
            deleted = ?3 \
         WHERE {predicate}"
    )
}

/// The `WHERE` clause addressing one row by its primary key.
pub fn pk_predicate(pk: PkSpec, first_param: usize) -> String {
    match pk {
        PkSpec::Single(col) => format!("{col} = ?{first_param}"),
        PkSpec::Composite(a, b) => {
            format!("{a} = ?{first_param} AND {b} = ?{}", first_param + 1)
        }
    }
}

/// Insert a junction row, or revive it if it was tombstoned.
///
/// An upsert rather than `INSERT OR IGNORE`: re-adding a membership that was
/// removed has to clear the tombstone, and an ignored insert would silently
/// leave it deleted while appearing to succeed.
pub fn upsert_junction(table: &str, col_a: &str, col_b: &str) -> String {
    format!(
        "INSERT INTO {table} ({col_a}, {col_b}, updated_at, updated_by, deleted) \
         VALUES (?1, ?2, ?3, ?4, 0) \
         ON CONFLICT({col_a}, {col_b}) DO UPDATE SET \
            deleted = 0, updated_at = ?3, updated_by = ?4"
    )
}

/// Tombstone every row of a table belonging to one paper.
///
/// Used by the delete cascade. Tombstoned rather than deleted: a hard delete
/// leaves nothing to publish, so a peer still holding the child row would treat
/// its copy as news and resurrect it.
pub fn tombstone_children(table: &str, column: &str) -> String {
    format!(
        "UPDATE {table} SET deleted = 1, updated_at = ?2, updated_by = ?3 \
         WHERE {column} = ?1"
    )
}

/// Tombstone a paper's citation edges in both directions.
pub fn tombstone_citations() -> &'static str {
    "UPDATE paper_citations SET deleted = 1, updated_at = ?2, updated_by = ?3 \
     WHERE citing_paper_id = ?1 OR cited_paper_id = ?1"
}

/// Read every column a snapshot carries for one table.
///
/// Reads the base table, not its `_live` view: a snapshot has to carry
/// tombstones or a deletion would never reach another device.
pub fn select_snapshot_rows(table: &SyncedTable) -> String {
    format!(
        "SELECT {} FROM {}",
        table.all_columns().join(", "),
        table.name
    )
}

/// Apply one peer row, keeping it only if its clock beats the local one.
///
/// The `WHERE` on `DO UPDATE` is what makes the merge idempotent and stops an
/// older peer row from clobbering a newer local edit. Without it, the last
/// writer to *arrive* would win, which is not the last writer to *edit*.
pub fn merge_row(table: &str, key_cols: &[&str], payload_cols: &[&str]) -> String {
    let mut insert_cols: Vec<&str> = key_cols.to_vec();
    insert_cols.extend_from_slice(payload_cols);
    insert_cols.extend_from_slice(SYNC_COLUMNS);

    let placeholders: Vec<String> = (1..=insert_cols.len()).map(|i| format!("?{i}")).collect();

    let mut assignments: Vec<String> = payload_cols
        .iter()
        .map(|c| format!("{c} = excluded.{c}"))
        .collect();
    for c in SYNC_COLUMNS {
        assignments.push(format!("{c} = excluded.{c}"));
    }

    format!(
        "INSERT INTO {table} ({cols}) VALUES ({vals}) \
         ON CONFLICT({conflict}) DO UPDATE SET {sets} \
         WHERE excluded.updated_at > {table}.updated_at \
            OR (excluded.updated_at = {table}.updated_at \
                AND excluded.updated_by > {table}.updated_by)",
        cols = insert_cols.join(", "),
        vals = placeholders.join(", "),
        conflict = key_cols.join(", "),
        sets = assignments.join(", "),
    )
}

/// Permanently remove this device's settled tombstones from one table.
///
/// The only statement here that destroys data. Both bounds are the caller's to
/// supply — see [`Database::reap_tombstones`](crate::Database::reap_tombstones)
/// for why each is needed.
pub fn reap_tombstones(table: &str) -> String {
    format!("DELETE FROM {table} WHERE deleted = 1 AND updated_at < ?1 AND updated_by = ?2")
}

/// Count rows in a table matching a predicate.
pub fn count_where(table: &str, predicate: &str) -> String {
    format!("SELECT COUNT(*) FROM {table} WHERE {predicate}")
}

/// A table's columns, for checking the manifest against the database.
pub fn table_info(table: &str) -> String {
    format!("PRAGMA table_info({table})")
}

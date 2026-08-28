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
///
/// The update branch clamps `updated_at` the way [`stamp_row`] does. Only that
/// branch: on insert there is no prior row to outrank, and `{table}.updated_at`
/// would not resolve.
pub fn upsert_junction(table: &str, col_a: &str, col_b: &str) -> String {
    format!(
        "INSERT INTO {table} ({col_a}, {col_b}, updated_at, updated_by, deleted) \
         VALUES (?1, ?2, ?3, ?4, 0) \
         ON CONFLICT({col_a}, {col_b}) DO UPDATE SET \
            deleted = 0, \
            updated_at = MAX(?3, {table}.updated_at + 1), \
            updated_by = ?4"
    )
}

/// Tombstone every row of a table belonging to one paper.
///
/// Used by the delete cascade. Tombstoned rather than deleted: a hard delete
/// leaves nothing to publish, so a peer still holding the child row would treat
/// its copy as news and resurrect it.
///
/// `updated_at` is clamped as in [`stamp_row`]: a child a peer stamped ahead of
/// this device's clock would otherwise outrank the tombstone meant to retire
/// it, and the paper would come back one child at a time.
pub fn tombstone_children(table: &str, column: &str) -> String {
    format!(
        "UPDATE {table} SET deleted = 1, \
            updated_at = MAX(?2, updated_at + 1), updated_by = ?3 \
         WHERE {column} = ?1"
    )
}

/// Tombstone a paper's citation edges in both directions.
///
/// Clamped like [`tombstone_children`], and for the same reason.
pub fn tombstone_citations() -> &'static str {
    "UPDATE paper_citations SET deleted = 1, \
        updated_at = MAX(?2, updated_at + 1), updated_by = ?3 \
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

/// Apply one peer row column by column, each judged on its own clock.
///
/// The row-level [`merge_row`] gates the whole row on one `WHERE`, so a peer row
/// that loses discards every column — including ones it changed and this device
/// did not. Here the gate moves inside each assignment, as a `CASE` over that
/// column's own clock, and there is no outer `WHERE` at all.
///
/// `graded_cols` are the columns to judge; `insert_only_cols` are inserted to
/// satisfy NOT NULL but never assigned. **That split is load-bearing.** A
/// tombstone carries no payload, so the caller supplies fabricated placeholders
/// (`title = ''`) for the insert path. Judging those would let an empty string
/// win a live column, and because the win writes the clock alongside the value,
/// the real one would be unrecoverable. Placeholders may create a row; they may
/// never modify one.
///
/// `deleted` stays row-level and intentionally so: a tombstone has no per-column
/// clocks to compare, so the row clock is the only thing that can order it, and
/// the reaper filters on `deleted = 1 AND updated_at < ?`. A tombstone's column
/// clocks are therefore never read, which is why a column clock sitting a
/// millisecond ahead of the row clock — as the write path's clamp can produce —
/// cannot make the reaper destroy anything a peer still needs.
pub fn merge_row_per_column(
    table: &str,
    key_cols: &[&str],
    graded_cols: &[&str],
    insert_only_cols: &[&str],
) -> String {
    let mut insert_cols: Vec<String> = key_cols.iter().map(|c| (*c).to_string()).collect();
    for c in graded_cols {
        insert_cols.push((*c).to_string());
        insert_cols.push(format!("{c}_ua"));
        insert_cols.push(format!("{c}_ub"));
    }
    insert_cols.extend(insert_only_cols.iter().map(|c| (*c).to_string()));
    insert_cols.extend(SYNC_COLUMNS.iter().map(|c| (*c).to_string()));

    let placeholders: Vec<String> = (1..=insert_cols.len()).map(|i| format!("?{i}")).collect();

    // One column wins when its own clock is newer, with the device id as the
    // deterministic tiebreak — the same comparison the row-level merge makes,
    // applied per column.
    let wins = |c: &str| {
        format!(
            "excluded.{c}_ua > {table}.{c}_ua \
             OR (excluded.{c}_ua = {table}.{c}_ua AND excluded.{c}_ub > {table}.{c}_ub)"
        )
    };
    let row_wins = format!(
        "excluded.updated_at > {table}.updated_at \
         OR (excluded.updated_at = {table}.updated_at \
             AND excluded.updated_by > {table}.updated_by)"
    );

    let mut assignments: Vec<String> = Vec::new();
    for c in graded_cols {
        let w = wins(c);
        assignments.push(format!(
            "{c} = CASE WHEN {w} THEN excluded.{c} ELSE {table}.{c} END"
        ));
        // MAX rather than a CASE: the clock only ever moves forward, and both
        // sides are NOT NULL so scalar MAX cannot yield NULL here.
        assignments.push(format!("{c}_ua = MAX({table}.{c}_ua, excluded.{c}_ua)"));
        assignments.push(format!(
            "{c}_ub = CASE WHEN {w} THEN excluded.{c}_ub ELSE {table}.{c}_ub END"
        ));
    }

    // The row clock envelopes the column clocks, so it never moves backwards.
    assignments.push(format!(
        "updated_at = MAX({table}.updated_at, excluded.updated_at)"
    ));
    assignments.push(format!(
        "updated_by = CASE WHEN {row_wins} THEN excluded.updated_by ELSE {table}.updated_by END"
    ));
    assignments.push(format!(
        "deleted = CASE WHEN {row_wins} THEN excluded.deleted ELSE {table}.deleted END"
    ));

    format!(
        "INSERT INTO {table} ({cols}) VALUES ({vals}) \
         ON CONFLICT({conflict}) DO UPDATE SET {sets}",
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

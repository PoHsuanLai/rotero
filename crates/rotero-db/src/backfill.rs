//! One-time repair for libraries written without CRR change tracking.
//!
//! A shipped build opened the database without calling `crr.init()`, so every
//! write committed its row and then failed to record a clock entry. Adding the
//! call back creates the clock tables, but they start empty: rows written during
//! that period have no clock entries at all, and `changes_since` reads
//! exclusively from those tables. Without a repair those rows would stay on the
//! machine that created them forever.
//!
//! `recrr`'s own `migrate_add_column` cannot do this. It finds rows by reading
//! sentinel entries *out of the clock table*, so on an affected library — where
//! the clock table is empty — it selects nothing. It repairs a missing column on
//! already-tracked rows, not rows that were never tracked.
//!
//! So this adopts untracked rows directly, seeding exactly the clock state a
//! correct insert would have produced.

use crate::Database;
use recrr::PkSpec;

/// Marks a row as alive in its clock table. Mirrors `recrr`'s private constant;
/// the sentinel name is part of the on-disk format, not an implementation
/// detail we are free to choose.
const SENTINEL: &str = "__sentinel";

/// `app_flags` key recording that the repair has run.
const BACKFILL_FLAG: &str = "crr_backfill_v1";

/// The shadow clock table for a tracked table.
fn clock_table(table: &str) -> String {
    format!("{table}__crr_clock")
}

/// SQL selecting the primary keys of rows that have no clock entry.
///
/// Rows that arrived via sync already have a sentinel and are skipped; rows a
/// peer deleted are gone from the real table and never appear. Composite keys
/// are concatenated with the schema's own separator so the value matches what
/// `track_insert` and the merge path expect.
fn untracked_pk_query(table: &str, pk: &PkSpec) -> String {
    let clock = clock_table(table);
    let pk_expr = match pk {
        PkSpec::Single { column } => format!("t.{column}"),
        PkSpec::Composite { columns, sep } => {
            format!("t.{} || '{}' || t.{}", columns.0, sep, columns.1)
        }
    };

    format!(
        "SELECT {pk_expr} FROM {table} t
         LEFT JOIN {clock} c ON c.pk = {pk_expr} AND c.col_name = '{SENTINEL}'
         WHERE c.pk IS NULL"
    )
}

impl Database {
    /// Whether the one-time CRR backfill has already run for this library.
    async fn backfill_done(&self) -> bool {
        let Ok(mut rows) = self
            .conn()
            .query(
                "SELECT value FROM app_flags WHERE key = ?1",
                [turso::Value::Text(BACKFILL_FLAG.to_string())],
            )
            .await
        else {
            // Treat an unreadable flag as "already done": re-running the scan is
            // harmless, but claiming it succeeded when the database is unhealthy
            // would be worse.
            return true;
        };
        matches!(rows.next().await, Ok(Some(_)))
    }

    async fn mark_backfill_done(&self) -> Result<(), turso::Error> {
        self.conn()
            .execute(
                "INSERT OR REPLACE INTO app_flags (key, value) VALUES (?1, '1')",
                [turso::Value::Text(BACKFILL_FLAG.to_string())],
            )
            .await?;
        Ok(())
    }

    /// Adopt rows that exist in the app tables but were never change-tracked.
    ///
    /// Runs at most once per library, gated on an `app_flags` row, so a healthy
    /// database pays one indexed query per table on a single launch and nothing
    /// thereafter. Returns the number of rows adopted.
    ///
    /// Each adopted row is seeded exactly as [`track_insert`] would have: a live
    /// sentinel plus every tracked column at `col_ver = 1`. A genuine later edit
    /// (`col_ver >= 2`) therefore always wins the LWW comparison against a
    /// backfilled value.
    ///
    /// [`track_insert`]: recrr::Crr::track_insert
    pub(crate) async fn backfill_untracked_rows(&self) -> Result<usize, String> {
        if self.backfill_done().await {
            return Ok(0);
        }

        let schema = crate::crr::rotero_schema();
        let mut total = 0usize;

        for table in &schema.tables {
            let sql = untracked_pk_query(&table.name, &table.pk);
            let mut rows = match self.conn().query(&sql, ()).await {
                Ok(rows) => rows,
                Err(e) => {
                    // A table missing here means the schema is broken in a way
                    // the health check reports; skip rather than abort, so the
                    // remaining tables are still repaired.
                    tracing::warn!("Backfill: skipping {}: {e}", table.name);
                    continue;
                }
            };

            let mut pks: Vec<String> = Vec::new();
            while let Ok(Some(row)) = rows.next().await {
                if let Ok(v) = row.get_value(0)
                    && let Some(pk) = v.as_text()
                {
                    pks.push(pk.clone());
                }
            }

            if pks.is_empty() {
                continue;
            }

            let columns: Vec<&str> = table.columns.iter().map(String::as_str).collect();
            for pk in &pks {
                self.crr()
                    .track_insert(&table.name, pk, &columns)
                    .await
                    .map_err(|e| format!("Failed to adopt {} row {pk}: {e}", table.name))?;
            }

            tracing::info!(
                "Backfill: adopted {} untracked row(s) in {}",
                pks.len(),
                table.name
            );
            total += pks.len();
        }

        self.mark_backfill_done()
            .await
            .map_err(|e| format!("Failed to record backfill completion: {e}"))?;

        if total > 0 {
            tracing::info!("Backfill: adopted {total} row(s) that were not being synced");
        }
        Ok(total)
    }
}

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
//! So this adopts rows directly via [`track_adopt`], which decides per row
//! whether adoption is needed and what clock state it should land in.
//!
//! [`track_adopt`]: recrr::Crr::track_adopt

use crate::Database;
use recrr::PkSpec;

/// `app_flags` key recording that the repair has run.
///
/// `v1` selected only rows with no clock entry at all, which silently skipped a
/// row that had been deleted and then re-created while tracking was broken —
/// `track_delete` leaves the sentinel in place at an even value, so such a row
/// looks tracked while reading as deleted to every peer. Libraries stamped by
/// that version still need repairing, so this key is bumped to make them
/// re-scan once.
const BACKFILL_FLAG: &str = "crr_backfill_v2";

/// SQL selecting every primary key in a table.
///
/// Deliberately unfiltered: deciding which rows need adoption requires reading
/// the sentinel's parity, which `track_adopt` does per row. Filtering here on
/// the mere *existence* of a clock entry is what made the first version skip
/// re-created rows. Composite keys are concatenated with the schema's own
/// separator so the value matches what the tracking and merge paths expect.
fn all_pk_query(table: &str, pk: &PkSpec) -> String {
    let pk_expr = match pk {
        PkSpec::Single { column } => column.clone(),
        PkSpec::Composite { columns, sep } => {
            format!("{} || '{}' || {}", columns.0, sep, columns.1)
        }
    };

    format!("SELECT {pk_expr} FROM {table}")
}

impl Database {
    /// The sentinel clock for a row, or 0 when it has none.
    ///
    /// Used only to tell whether an adoption actually changed anything, so the
    /// log reports repaired rows rather than every row examined. Returns 0 on a
    /// read error, which makes an unreadable clock look unchanged — the count is
    /// diagnostic, and undercounting it is better than aborting the repair.
    async fn sentinel_col_ver(&self, table: &str, pk: &str) -> i64 {
        let sql = format!(
            "SELECT col_ver FROM {table}__crr_clock WHERE pk = ?1 AND col_name = '__sentinel'"
        );
        let Ok(mut rows) = self
            .conn()
            .query(&sql, [turso::Value::Text(pk.to_string())])
            .await
        else {
            return 0;
        };
        match rows.next().await {
            Ok(Some(row)) => row
                .get_value(0)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0),
            _ => 0,
        }
    }

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

    /// Adopt rows that exist in the app tables but are not correctly tracked.
    ///
    /// Runs at most once per library, gated on an `app_flags` row, so a healthy
    /// database pays one scan per table on a single launch and nothing
    /// thereafter. Returns the number of rows actually adopted.
    ///
    /// Every row is handed to [`track_adopt`], which is a no-op for one that is
    /// already alive and tracked. Deciding here instead would mean reproducing
    /// the sentinel's parity rule in SQL, which is exactly the mistake the first
    /// version made.
    ///
    /// Adopted rows are seeded at `col_ver = 1`, so a genuine later edit
    /// (`col_ver >= 2`) always wins the LWW comparison against a backfilled
    /// value.
    ///
    /// [`track_adopt`]: recrr::Crr::track_adopt
    pub(crate) async fn backfill_untracked_rows(&self) -> Result<usize, String> {
        if self.backfill_done().await {
            return Ok(0);
        }

        let schema = crate::crr::rotero_schema();
        let mut total = 0usize;

        for table in &schema.tables {
            let sql = all_pk_query(&table.name, &table.pk);
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
            let mut adopted = 0usize;
            for pk in &pks {
                // `changes_since` only reports rows the clock knows about, so
                // count the ones that were genuinely repaired rather than every
                // row offered.
                let before = self.sentinel_col_ver(&table.name, pk).await;
                self.crr()
                    .track_adopt(&table.name, pk, &columns)
                    .await
                    .map_err(|e| format!("Failed to adopt {} row {pk}: {e}", table.name))?;
                if self.sentinel_col_ver(&table.name, pk).await != before {
                    adopted += 1;
                }
            }

            if adopted > 0 {
                tracing::info!(
                    "Backfill: adopted {adopted} untracked row(s) in {}",
                    table.name
                );
            }
            total += adopted;
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

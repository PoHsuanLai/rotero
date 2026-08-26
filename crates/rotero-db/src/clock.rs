//! Stamping local writes so they can win a merge.
//!
//! Every local mutation records when it happened and which device did it. The
//! merge compares `(updated_at, updated_by)` as a tuple, so a row that is never
//! stamped loses every comparison forever — present locally, unable to
//! propagate. That is the failure this module exists to make hard: writes go
//! through [`Database::touch`], [`Database::tombstone`], and
//! [`Database::upsert_junction`] rather than each call site remembering to
//! stamp, which is how the old tracking API kept losing tags and notes.

use turso::Value;

use crate::Database;

/// What a peer changeset did when applied.
#[derive(Clone, Copy, Debug)]
pub struct ApplyOutcome {
    /// How many changes were new to this device.
    pub applied: usize,
    /// How many were already known and had no effect.
    pub skipped: usize,
}

/// A row's primary key, single or composite.
#[derive(Clone, Copy, Debug)]
pub enum Pk<'a> {
    /// A single `id` column.
    Single(&'a str),
    /// Two key columns, as the junction tables use.
    Composite(&'a str, &'a str),
}

impl Database {
    /// Stamp a row as modified by this device, now.
    ///
    /// Clamped to at least one millisecond past the row's existing stamp. Two
    /// writes inside the same millisecond would otherwise tie, and — more
    /// importantly — a clock that jumps backwards would let a device's own newer
    /// edit lose to its own older one, which no amount of merge determinism can
    /// repair afterwards.
    pub async fn touch(&self, table: &str, pk: Pk<'_>) -> Result<(), crate::DbError> {
        self.stamp(table, pk, false).await
    }

    /// Mark a row deleted, stamped so the deletion propagates.
    ///
    /// A tombstone rather than a `DELETE`: removing the row outright leaves
    /// nothing to publish, so a peer still holding it would treat its own copy
    /// as news and resurrect it on the next merge.
    pub async fn tombstone(&self, table: &str, pk: Pk<'_>) -> Result<(), crate::DbError> {
        self.stamp(table, pk, true).await
    }

    /// Insert a junction row, or revive it if it was previously tombstoned.
    ///
    /// Re-adding a tag that was removed has to clear `deleted`, not just insert:
    /// the row still exists, so a plain `INSERT OR IGNORE` would silently do
    /// nothing and leave the membership tombstoned.
    pub async fn upsert_junction(
        &self,
        table: &str,
        key_a: (&str, &str),
        key_b: (&str, &str),
    ) -> Result<(), crate::DbError> {
        let (col_a, val_a) = key_a;
        let (col_b, val_b) = key_b;
        let (now, device) = (self.now_millis(), self.device_id().to_string());

        self.conn()
            .execute(
                &format!(
                    "INSERT INTO {table} ({col_a}, {col_b}, updated_at, updated_by, deleted) \
                     VALUES (?1, ?2, ?3, ?4, 0) \
                     ON CONFLICT({col_a}, {col_b}) DO UPDATE SET \
                        deleted = 0, updated_at = ?3, updated_by = ?4"
                ),
                turso::params::Params::Positional(vec![
                    Value::Text(val_a.to_string()),
                    Value::Text(val_b.to_string()),
                    Value::Integer(now),
                    Value::Text(device),
                ]),
            )
            .await?;
        Ok(())
    }

    /// Write a row's sync clock, optionally tombstoning it.
    async fn stamp(&self, table: &str, pk: Pk<'_>, deleted: bool) -> Result<(), crate::DbError> {
        let device = self.device_id().to_string();
        let now = self.now_millis();

        let (predicate, key_values) = match pk {
            Pk::Single(id) => ("id = ?4".to_string(), vec![Value::Text(id.to_string())]),
            Pk::Composite(a, b) => {
                let table = crate::sync_schema::synced_table(table);
                let cols = table
                    .map(|t| t.pk.columns())
                    .unwrap_or_else(|| vec!["paper_id", "tag_id"]);
                (
                    format!("{} = ?4 AND {} = ?5", cols[0], cols[1]),
                    vec![Value::Text(a.to_string()), Value::Text(b.to_string())],
                )
            }
        };

        let mut params = vec![
            Value::Integer(now),
            Value::Text(device),
            Value::Integer(deleted as i64),
        ];
        params.extend(key_values);

        self.conn()
            .execute(
                &format!(
                    "UPDATE {table} SET \
                        updated_at = MAX(?1, updated_at + 1), \
                        updated_by = ?2, \
                        deleted = ?3 \
                     WHERE {predicate}"
                ),
                turso::params::Params::Positional(params),
            )
            .await?;
        Ok(())
    }

    /// Wall-clock milliseconds, the basis of every comparison.
    fn now_millis(&self) -> i64 {
        chrono::Utc::now().timestamp_millis()
    }
}

impl Database {
    /// Clear tombstones on rows that recrr has re-created.
    ///
    /// Transitional. While both systems run, recrr applies changes by writing
    /// row values directly and knows nothing about `deleted`, so a row it
    /// resurrects stays tombstoned and remains invisible behind the `_live`
    /// views. Reads would keep hiding a row the sync engine believes is alive.
    ///
    /// recrr's own liveness lives in its clock tables (an odd `cl` means alive),
    /// so rather than reach into those, this clears the tombstone on any row
    /// recrr reports as present. It goes away with recrr in step 12 — the
    /// snapshot merge carries `deleted` as an ordinary column and needs no
    /// reconciliation.
    /// Apply a peer changeset, keeping `deleted` consistent with it.
    ///
    /// The transports and tests go through here rather than calling
    /// `crr().apply_changes` directly, so a resurrected row is visible again
    /// instead of staying hidden behind the `_live` views. Transitional, and
    /// removed with recrr.
    pub async fn apply_changes(
        &self,
        changes: &[crate::crr::ChangeRow],
    ) -> Result<ApplyOutcome, crate::DbError> {
        let outcome = self.crr().apply_changes(changes).await?;

        let touched: Vec<(String, String)> = changes
            .iter()
            .map(|c| (c.table_name.clone(), c.pk.clone()))
            .collect();
        self.reconcile_tombstones_after_crr_apply(&touched).await?;

        Ok(ApplyOutcome {
            applied: outcome.applied,
            skipped: outcome.skipped,
        })
    }

    pub async fn reconcile_tombstones_after_crr_apply(
        &self,
        rows: &[(String, String)],
    ) -> Result<(), crate::DbError> {
        for (table, pk) in rows {
            let Some(spec) = crate::sync_schema::synced_table(table) else {
                continue;
            };
            let predicate = match spec.pk {
                crate::sync_schema::PkSpec::Single(col) => format!("{col} = ?1"),
                crate::sync_schema::PkSpec::Composite(a, b) => {
                    // recrr addresses composite keys as "a:b".
                    let Some((_, _)) = pk.split_once(':') else {
                        continue;
                    };
                    format!("{a} || ':' || {b} = ?1")
                }
            };
            self.conn()
                .execute(
                    &format!("UPDATE {table} SET deleted = 0 WHERE {predicate}"),
                    [Value::Text(pk.clone())],
                )
                .await?;
        }
        Ok(())
    }
}

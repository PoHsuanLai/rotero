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
                &crate::sync_sql::upsert_junction(table, col_a, col_b),
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

        // The manifest is the authority on a table's key columns. A table it
        // does not know is not synced, so stamping it would write a clock
        // nothing ever reads — and guessing the key columns could stamp the
        // wrong row entirely.
        let Some(spec) = crate::sync_schema::synced_table(table) else {
            return Ok(());
        };

        let predicate = crate::sync_sql::pk_predicate(spec.pk, 4);
        let key_values = match pk {
            Pk::Single(id) => vec![Value::Text(id.to_string())],
            Pk::Composite(a, b) => {
                vec![Value::Text(a.to_string()), Value::Text(b.to_string())]
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
                &crate::sync_sql::stamp_row(table, &predicate),
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

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

    /// Stamp a row, and the individual columns this write changed.
    ///
    /// On a per-column table the merge judges each column by its own clock, so a
    /// write has to say which columns it touched. The row clock is stamped too
    /// and stays an envelope over the column clocks — the reaper and the health
    /// check still read it, and a column clock ahead of the row clock would let
    /// the reaper destroy a row carrying a newer column.
    ///
    /// `columns` must list exactly the columns whose value changed. Both errors
    /// are silent: a column left out never propagates while its siblings do, and
    /// one wrongly included stamps a value this device did not write, so a stale
    /// local copy outranks and destroys a peer's real edit. Nothing at runtime
    /// catches either — `write_statement_columns_match_their_call_sites` in
    /// `sync_schema_test.rs` is what makes this safe to get wrong.
    ///
    /// On a row-level table this is exactly [`touch`](Self::touch); passing an
    /// empty slice is the deliberate way to stamp only the row, which is what a
    /// write to a non-synced column (`citation_key`) or a junction needs.
    pub async fn touch_columns(
        &self,
        table: &str,
        pk: Pk<'_>,
        columns: &[&str],
    ) -> Result<(), crate::DbError> {
        self.stamp(table, pk, false).await?;

        let per_column = crate::sync_schema::synced_table(table).is_some_and(|t| t.per_column);
        if !per_column || columns.is_empty() {
            return Ok(());
        }

        let now = self.now_millis();
        let device = self.device_id().to_string();

        // Clamped the same way `stamp_row` clamps the row clock: two writes in
        // one millisecond would otherwise tie, and a backwards clock would let
        // this device's own newer edit lose to its own older one.
        let sets: Vec<String> = columns
            .iter()
            .map(|c| format!("{c}_ua = MAX(?1, {c}_ua + 1), {c}_ub = ?2"))
            .collect();

        // Only single-keyed tables can be per-column: the composite-keyed
        // tables are the junctions, which have no payload columns at all, so
        // there is nothing for a per-column clock to describe.
        let Pk::Single(id) = pk else {
            return Ok(());
        };

        let params = vec![
            Value::Integer(now),
            Value::Text(device),
            Value::Text(id.to_string()),
        ];

        self.conn()
            .execute(
                &format!("UPDATE {table} SET {} WHERE id = ?3", sets.join(", ")),
                turso::params::Params::Positional(params),
            )
            .await?;
        Ok(())
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

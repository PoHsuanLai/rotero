//! Serializing a device's synced tables, and merging a peer's.
//!
//! Each device writes one snapshot of its own rows and reads every other
//! device's. Because no two devices ever write the same file there are no write
//! conflicts to resolve — only the question of which copy of a row is newest,
//! which `(updated_at, updated_by)` answers deterministically.
//!
//! The format is newline-delimited JSON, gzipped, with a header line: readable
//! with `gunzip -c | head` when sync misbehaves, and tolerant of truncation in
//! the sense that a short read fails to parse rather than silently yielding
//! half a library.

use std::collections::BTreeMap;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use turso::Value;

use crate::Database;
use crate::sync_schema::{PkSpec, SYNCED_TABLES, SyncedTable};

/// Bumped when the on-disk shape changes incompatibly.
pub const FORMAT_VERSION: u32 = 1;

/// The first line of a snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHeader {
    /// Format version, so a future change is detectable rather than misread.
    pub format: u32,
    /// Which device wrote this file.
    pub site_id: String,
    /// When it was written, unix millis. Also used to spot a badly skewed peer.
    pub generated_at: i64,
    /// How many rows follow, so a truncated file fails loudly.
    pub rows: usize,
}

/// One row of one table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRow {
    /// Table name.
    pub t: String,
    /// Primary key values, in the order the manifest declares them.
    pub k: Vec<String>,
    /// Non-key column values, keyed by column name. Absent for a tombstone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub v: Option<BTreeMap<String, serde_json::Value>>,
    /// Unix millis of the last write.
    pub ua: i64,
    /// Device that made it.
    pub ub: String,
    /// Tombstone flag.
    pub d: bool,
}

/// What merging a peer snapshot changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MergeStats {
    /// Rows that were new or newer than the local copy.
    pub applied: usize,
    /// Rows the local copy already matched or beat.
    pub skipped: usize,
}

/// Why a snapshot could not be read.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// The bytes are not a readable snapshot.
    #[error("malformed snapshot: {0}")]
    Malformed(String),
    /// Written by a newer build than this one understands.
    #[error("snapshot format v{found} is newer than v{supported}")]
    NewerFormat {
        /// The version found in the file.
        found: u32,
        /// The newest version this build reads.
        supported: u32,
    },
    /// A database error while reading or applying rows.
    #[error(transparent)]
    Db(#[from] crate::DbError),
}

impl Database {
    /// Serialize this device's synced tables.
    ///
    /// Reads the base tables, not the `_live` views: a snapshot has to carry
    /// tombstones, or a deletion would never reach the other device.
    pub async fn write_snapshot(&self) -> Result<Vec<u8>, SnapshotError> {
        let mut rows = Vec::new();
        for table in SYNCED_TABLES {
            rows.extend(self.read_table(table).await?);
        }

        let header = SnapshotHeader {
            format: FORMAT_VERSION,
            site_id: self.device_id().to_string(),
            generated_at: chrono::Utc::now().timestamp_millis(),
            rows: rows.len(),
        };

        let mut plain = Vec::new();
        writeln!(plain, "{}", serde_json::to_string(&header).unwrap())
            .map_err(|e| SnapshotError::Malformed(e.to_string()))?;
        for row in &rows {
            writeln!(plain, "{}", serde_json::to_string(row).unwrap())
                .map_err(|e| SnapshotError::Malformed(e.to_string()))?;
        }

        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&plain)
            .map_err(|e| SnapshotError::Malformed(e.to_string()))?;
        encoder
            .finish()
            .map_err(|e| SnapshotError::Malformed(e.to_string()))
    }

    /// Merge a peer's snapshot into this device's tables.
    ///
    /// Every row is applied with one conditional upsert, so the result does not
    /// depend on the order peer files are read in and re-applying a snapshot
    /// changes nothing.
    pub async fn merge_snapshot(&self, bytes: &[u8]) -> Result<MergeStats, SnapshotError> {
        let (header, rows) = parse_snapshot(bytes)?;

        // Skip our own snapshot. This is an optimisation, not a correctness
        // requirement — the clock guard already rejects every row, since a
        // device's own rows can never beat their own timestamps. Removing it
        // changes no observable behaviour, only the amount of work done, which
        // is why no test asserts on it.
        if header.site_id == self.device_id() {
            return Ok(MergeStats::default());
        }

        let mut stats = MergeStats::default();
        for mut row in rows {
            let Some(table) = crate::sync_schema::synced_table(&row.t) else {
                // A table this build does not know about: a newer peer may sync
                // more than we do. Skip rather than fail the whole file.
                stats.skipped += 1;
                continue;
            };
            // `tags.name` is UNIQUE across live and dead rows alike, so any
            // incoming tag can collide with a local one holding that name.
            if table.name == "tags" {
                self.reconcile_tag_name(&mut row).await?;
            }
            if self.apply_row(table, &row).await? {
                stats.applied += 1;
            } else {
                stats.skipped += 1;
            }
        }
        Ok(stats)
    }

    /// Read one table's rows into snapshot form.
    async fn read_table(&self, table: &SyncedTable) -> Result<Vec<SnapshotRow>, SnapshotError> {
        let sql = crate::sync_sql::select_snapshot_rows(table);
        let mut rows = self
            .conn()
            .query(&sql, ())
            .await
            .map_err(crate::DbError::from)?;

        let key_len = table.pk.columns().len();
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.map_err(crate::DbError::from)? {
            let mut key = Vec::with_capacity(key_len);
            for i in 0..key_len {
                key.push(text_at(&row, i));
            }

            // Payload columns, then the per-column clocks that describe them.
            // Both come out of `all_columns()` in the order `select_snapshot_rows`
            // asked for, so the offsets cannot drift from the SELECT.
            let mut values = BTreeMap::new();
            let clock_cols = table.clock_columns();
            let payload = table.columns.iter().chain(clock_cols.iter());
            for (offset, name) in payload.enumerate() {
                let v = row
                    .get_value(key_len + offset)
                    .map_err(crate::DbError::from)?;
                values.insert((*name).to_string(), to_json(&v));
            }

            let sync_base = key_len + table.columns.len() + clock_cols.len();
            let ua = int_at(&row, sync_base);
            let ub = text_at(&row, sync_base + 1);
            let d = int_at(&row, sync_base + 2) != 0;

            out.push(SnapshotRow {
                t: table.name.to_string(),
                k: key,
                // A tombstone carries no payload: the key and the clock are all
                // a peer needs, and it keeps reaped-pending rows nearly free.
                v: (!d).then_some(values),
                ua,
                ub,
                d,
            });
        }
        Ok(out)
    }

    /// Apply one peer row, returning whether it won.
    async fn apply_row(
        &self,
        table: &SyncedTable,
        row: &SnapshotRow,
    ) -> Result<bool, SnapshotError> {
        let key_cols = table.pk.columns();
        if row.k.len() != key_cols.len() {
            return Err(SnapshotError::Malformed(format!(
                "`{}` expects {} key column(s), row has {}",
                table.name,
                key_cols.len(),
                row.k.len()
            )));
        }

        // A tombstone carries no payload. For a row we already have that is
        // fine — only the clock and the flag change. But a tombstone can also
        // be the first thing we ever hear about a row (a paper created and
        // deleted before we last synced), and inserting one still has to
        // satisfy the table's NOT NULL columns. Those get empty placeholders:
        // the row is dead, nothing reads it, and it exists only to carry the
        // deletion onward to a third device.
        let payload_cols: Vec<&str> = if row.d {
            Vec::new()
        } else {
            table.columns.to_vec()
        };

        let mut insert_cols: Vec<&str> = key_cols.clone();
        insert_cols.extend(payload_cols.iter().copied());
        let skeleton: Vec<(&str, Value)> = if row.d {
            not_null_skeleton(
                table.name,
                row.k.first().map(String::as_str).unwrap_or_default(),
            )
        } else {
            Vec::new()
        };

        let empty = BTreeMap::new();
        let values = row.v.as_ref().unwrap_or(&empty);

        let (sql, mut params) = if table.per_column {
            // Skeleton columns are inserted but never graded — see
            // `merge_row_per_column`. Judging a fabricated placeholder would let
            // an empty string win a live column and write its clock, after which
            // the real value is unrecoverable.
            let skeleton_cols: Vec<&str> = skeleton.iter().map(|(c, _)| *c).collect();
            let sql = crate::sync_sql::merge_row_per_column(
                table.name,
                &key_cols,
                &payload_cols,
                &skeleton_cols,
            );

            let mut params: Vec<Value> = row.k.iter().map(|k| Value::Text(k.clone())).collect();
            for c in &payload_cols {
                params.push(from_json(
                    values.get(*c).unwrap_or(&serde_json::Value::Null),
                ));
                let (at, by) = column_clock(values, c, row);
                params.push(Value::Integer(at));
                params.push(Value::Text(by));
            }
            for (_, v) in &skeleton {
                params.push(v.clone());
            }
            (sql, params)
        } else {
            let mut generated_cols: Vec<&str> = payload_cols.clone();
            generated_cols.extend(skeleton.iter().map(|(c, _)| *c));
            let sql = crate::sync_sql::merge_row(table.name, &key_cols, &generated_cols);

            let mut params: Vec<Value> = row.k.iter().map(|k| Value::Text(k.clone())).collect();
            for c in &payload_cols {
                params.push(from_json(
                    values.get(*c).unwrap_or(&serde_json::Value::Null),
                ));
            }
            for (_, v) in &skeleton {
                params.push(v.clone());
            }
            (sql, params)
        };

        params.push(Value::Integer(row.ua));
        params.push(Value::Text(row.ub.clone()));
        params.push(Value::Integer(row.d as i64));

        let changed = self
            .conn()
            .execute(&sql, turso::params::Params::Positional(params))
            .await
            .map_err(crate::DbError::from)?;

        Ok(changed > 0)
    }
}

/// Split a snapshot into its header and rows.
///
/// A row that fails to parse is an error for the whole file rather than a
/// silent omission — half a peer's library applied is worse than none of it,
/// because the caller cannot tell the difference.
pub fn parse_snapshot(bytes: &[u8]) -> Result<(SnapshotHeader, Vec<SnapshotRow>), SnapshotError> {
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut plain)
        .map_err(|e| SnapshotError::Malformed(format!("not gzip: {e}")))?;

    let text = String::from_utf8(plain)
        .map_err(|e| SnapshotError::Malformed(format!("not utf-8: {e}")))?;
    let mut lines = text.lines();

    let header_line = lines
        .next()
        .ok_or_else(|| SnapshotError::Malformed("empty snapshot".into()))?;
    let header: SnapshotHeader = serde_json::from_str(header_line)
        .map_err(|e| SnapshotError::Malformed(format!("bad header: {e}")))?;

    if header.format > FORMAT_VERSION {
        return Err(SnapshotError::NewerFormat {
            found: header.format,
            supported: FORMAT_VERSION,
        });
    }

    let mut rows = Vec::with_capacity(header.rows);
    for (n, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str(line)
                .map_err(|e| SnapshotError::Malformed(format!("row {}: {e}", n + 1)))?,
        );
    }

    if rows.len() != header.rows {
        return Err(SnapshotError::Malformed(format!(
            "header promises {} rows, found {} — file is truncated",
            header.rows,
            rows.len()
        )));
    }

    Ok((header, rows))
}

/// The snapshot's SHA-256, written alongside it so a partly-uploaded file is
/// detectable rather than merged.
pub fn checksum(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// One column's clock, falling back to the row's when the peer sent none.
///
/// A device running an older schema publishes no per-column clocks. Reading a
/// missing one as zero would make every column of that peer's rows lose every
/// comparison — a total, silent sync failure between the two versions, which is
/// the worst outcome this design can produce.
///
/// Falling back to the row clock says exactly what row-level LWW meant: every
/// column was written when the row was. So an older peer degrades to the
/// previous behaviour, per column, correctly. The same path covers a column
/// added by a later schema version that this peer does not have yet.
fn column_clock(
    values: &BTreeMap<String, serde_json::Value>,
    column: &str,
    row: &SnapshotRow,
) -> (i64, String) {
    let at = values
        .get(&format!("{column}_ua"))
        .and_then(serde_json::Value::as_i64);
    let by = values
        .get(&format!("{column}_ub"))
        .and_then(serde_json::Value::as_str);
    match (at, by) {
        (Some(at), Some(by)) => (at, by.to_string()),
        // Partial clocks are treated as absent rather than half-trusted: a
        // timestamp without the device id cannot be tiebroken.
        _ => (row.ua, row.ub.clone()),
    }
}

fn text_at(row: &turso::Row, idx: usize) -> String {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default()
}

fn int_at(row: &turso::Row, idx: usize) -> i64 {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(0)
}

fn to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Integer(i) => serde_json::Value::from(*i),
        Value::Real(f) => serde_json::Value::from(*f),
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::Blob(b) => serde_json::Value::String(hex(b)),
    }
}

fn from_json(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Integer(*b as i64),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Integer)
            .or_else(|| n.as_f64().map(Value::Real))
            .unwrap_or(Value::Null),
        serde_json::Value::String(s) => Value::Text(s.clone()),
        other => Value::Text(other.to_string()),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Database {
    /// Resolve a same-name collision before applying an incoming tag.
    ///
    /// Two devices creating "ml" offline produce different ids for the same
    /// name, and `tags.name` is UNIQUE, so the second insert would fail. The
    /// survivor is `min(id)` rather than the newer one so that every device
    /// reaches the same answer regardless of the order peer files arrive in —
    /// which is what makes this converge without coordination.
    ///
    /// The loser is renamed only in the local table, never in what this device
    /// publishes: the rename is a local repair, and writing it into a synced
    /// column would make it a fact other devices have to agree about. Three
    /// devices that each created the tag would then each retire a different
    /// row, exchange those decisions, and disagree — and because a tombstone
    /// ships no payload, a retired name arrives as an empty string and
    /// overwrites a live one. Keeping the rename local means every device
    /// derives the same survivor from `min(id)`, which they can all compute.
    async fn reconcile_tag_name(&self, row: &mut SnapshotRow) -> Result<(), SnapshotError> {
        let incoming_id = row.k[0].clone();
        let incoming_id = incoming_id.as_str();
        // A tombstoned tag carries no payload, so there is no name to collide
        // with and nothing to reconcile.
        let Some(name) = row
            .v
            .as_ref()
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
        else {
            return Ok(());
        };

        let mut rows = self
            .conn()
            .query(
                crate::queries::TAG_FIND_NAME_CLASH,
                turso::params::Params::Positional(vec![
                    Value::Text(name.to_string()),
                    Value::Text(incoming_id.to_string()),
                ]),
            )
            .await
            .map_err(crate::DbError::from)?;

        let mut clashing = Vec::new();
        while let Some(r) = rows.next().await.map_err(crate::DbError::from)? {
            clashing.push(text_at(&r, 0));
        }
        // Drop the cursor before writing to the same table: turso keeps the
        // statement's view of the index alive while rows are still readable, so
        // renaming the loser under an open SELECT leaves the freed name still
        // claimed and the insert below fails on a constraint that no longer
        // reflects the data.
        drop(rows);

        if clashing.is_empty() {
            return Ok(());
        }

        // The survivor is the smallest id among everyone holding this name,
        // the incoming row included. Taking the minimum over the whole set
        // rather than comparing pairwise is what makes the outcome independent
        // of the order the peers' files happen to arrive in.
        let winner = clashing
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(incoming_id))
            .min()
            .expect("the set contains the incoming row")
            .to_string();

        let now = chrono::Utc::now().timestamp_millis();

        // When the incoming row is the one that loses, there is nothing local
        // to retire — it does not exist here yet — and the upsert below would
        // insert the name a local row already holds, failing the UNIQUE
        // constraint. A failed merge abandons the peer's whole snapshot, so
        // devices that each created the same tag name offline would stop
        // syncing altogether rather than merely disagreeing about one tag.
        //
        // Rename it as it lands instead. The row is still stored, so its clock
        // and any later deletion still travel onward to a third device; only
        // the name this device shows is local.
        if winner != incoming_id {
            if let Some(values) = row.v.as_mut() {
                values.insert(
                    "name".to_string(),
                    serde_json::Value::String(retired_tag_name(incoming_id)),
                );
            }
            return Ok(());
        }

        // The incoming row wins, so every local row holding the name has to
        // give it up — all of them, not just the first found: a third device
        // arriving late can find two local rows already sharing the name.
        for loser in clashing.iter().filter(|id| *id != &winner) {
            // Move memberships to the survivor before retiring the loser, so no
            // paper silently loses the tag.
            self.conn()
                .execute(
                    crate::queries::TAG_REPOINT_MEMBERSHIPS,
                    turso::params::Params::Positional(vec![
                        Value::Text(winner.clone()),
                        Value::Integer(now),
                        Value::Text(self.device_id().to_string()),
                        Value::Text(loser.clone()),
                    ]),
                )
                .await
                .map_err(crate::DbError::from)?;

            self.conn()
                .execute(
                    crate::queries::TAG_TOMBSTONE_MEMBERSHIPS,
                    turso::params::Params::Positional(vec![
                        Value::Integer(now),
                        Value::Text(self.device_id().to_string()),
                        Value::Text(loser.clone()),
                    ]),
                )
                .await
                .map_err(crate::DbError::from)?;

            // Free the name so the survivor can take it. `tags.name` is UNIQUE
            // across every row, dead ones included, so leaving it in place would
            // keep rejecting the survivor on every future merge and sync would
            // stall permanently on this one row.
            //
            // The sync clock is deliberately not stamped here. This rename is a
            // local repair, and stamping it would publish it: the retired name
            // would outrank the real one on every other device, and since a
            // tombstone carries no payload it would arrive as an empty string.
            // Left unstamped, each device repairs its own copy and they still agree
            // on which tag survives, because `min(id)` is the same everywhere.
            self.conn()
                .execute(
                    crate::queries::TAG_RETIRE_DUPLICATE,
                    turso::params::Params::Positional(vec![
                        Value::Text(retired_tag_name(loser)),
                        Value::Text(loser.clone()),
                    ]),
                )
                .await
                .map_err(crate::DbError::from)?;
        }

        Ok(())
    }
}

/// The placeholder name for a tag that cannot keep the one it came with.
///
/// Derived from the id so that it is unique. Both callers depend on that:
/// several tags can lose the same name, and several deleted tags can arrive
/// carrying none, and `tags.name` is UNIQUE across all of them.
pub fn retired_tag_name(id: &str) -> String {
    format!("__retired:{id}")
}

impl PkSpec {
    /// Whether this key is a single column.
    pub fn is_single(&self) -> bool {
        matches!(self, PkSpec::Single(_))
    }
}

/// Placeholder values for a table's NOT NULL columns.
///
/// Only used when a tombstone arrives for a row this device has never seen: the
/// insert must satisfy the schema even though nothing will ever read the row.
/// Kept deliberately minimal — a real value here would be a fabrication, and the
/// row is dead.
///
/// `tags.name` is the exception and takes the row's id. The constraint on it is
/// UNIQUE and spans dead rows, so a shared placeholder collides as soon as a
/// second deleted tag arrives: two tags deleted on two devices before either
/// synced would leave the second merge failing forever, and a failed merge
/// abandons the peer's whole snapshot.
fn not_null_skeleton(table: &str, id: &str) -> Vec<(&'static str, Value)> {
    let empty = |c| (c, Value::Text(String::new()));
    match table {
        "papers" => vec![
            ("title", Value::Text(String::new())),
            ("authors", Value::Text("[]".into())),
            empty("date_added"),
            empty("date_modified"),
            ("item_type", Value::Text("journalArticle".into())),
        ],
        "collections" => vec![empty("name")],
        "tags" => vec![("name", Value::Text(retired_tag_name(id)))],
        "annotations" => vec![
            empty("paper_id"),
            ("page", Value::Integer(0)),
            ("ann_type", Value::Text("note".into())),
            ("color", Value::Text("#ffff00".into())),
            ("geometry", Value::Text("{}".into())),
            empty("created_at"),
            empty("modified_at"),
        ],
        "notes" => vec![
            empty("paper_id"),
            empty("title"),
            empty("body"),
            empty("created_at"),
            empty("modified_at"),
        ],
        "saved_searches" => vec![empty("name"), empty("query"), empty("created_at")],
        _ => Vec::new(),
    }
}

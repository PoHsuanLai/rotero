//! CRR (Conflict-free Replicated Relations) change tracking and merge logic.
//!
//! Implements cr-sqlite-style CRDT semantics:
//! - Per-column LWW (Last-Writer-Wins) via version counters
//! - Causal length (CL) for delete/resurrect tracking
//! - Site ID for deterministic tie-breaking

use serde::{Deserialize, Serialize};
use turso::{Connection, Value};

/// A single column-level change record for sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRow {
    pub table_name: String,
    pub pk: String,
    pub col_name: String, // column name or "__sentinel" for row existence
    pub col_val: serde_json::Value,
    pub col_ver: i64,
    pub db_ver: i64,
    pub site_id: Vec<u8>, // 16-byte UUID
    pub seq: i64,
    pub cl: i64, // causal length (odd=alive, even=deleted)
}

/// Result of applying a changeset.
#[derive(Debug, Default)]
pub struct MergeResult {
    pub applied: usize,
    pub skipped: usize,
}

// ── Tables that participate in CRR ──────────────────────────────

/// All CRR-enabled tables and their non-PK columns.
const CRR_TABLES: &[(&str, &[&str])] = &[
    (
        "papers",
        &[
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
            // "fulltext" excluded — derived from PDF, re-extractable
            "citation_count",
            "citation_key",
        ],
    ),
    ("collections", &["name", "parent_id", "position"]),
    ("tags", &["name", "color"]),
    (
        "annotations",
        &[
            "paper_id",
            "page",
            "ann_type",
            "color",
            "content",
            "geometry",
            "created_at",
            "modified_at",
        ],
    ),
    (
        "notes",
        &[
            "paper_id",
            "title",
            "body",
            "created_at",
            "modified_at",
        ],
    ),
    ("saved_searches", &["name", "query", "created_at"]),
    ("paper_collections", &["paper_id", "collection_id"]),
    ("paper_tags", &["paper_id", "tag_id"]),
];

// ── Initialization ──────────────────────────────────────────────

/// Create CRR metadata tables and clock tables for all CRR-enabled tables (idempotent).
pub async fn init_crr_tables(conn: &Connection) -> Result<(), turso::Error> {
    // Metadata tables
    conn.execute(
        "CREATE TABLE IF NOT EXISTS crr_site_id (site_id BLOB PRIMARY KEY)",
        (),
    )
    .await?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS crr_db_version (version INTEGER NOT NULL)",
        (),
    )
    .await?;
    // Ensure there's a row in crr_db_version
    let mut rows = conn.query("SELECT version FROM crr_db_version LIMIT 1", ()).await?;
    if rows.next().await?.is_none() {
        conn.execute("INSERT INTO crr_db_version (version) VALUES (0)", ()).await?;
    }
    // Ensure there's a site_id
    let mut rows = conn.query("SELECT site_id FROM crr_site_id LIMIT 1", ()).await?;
    if rows.next().await?.is_none() {
        conn.execute(
            "INSERT INTO crr_site_id (site_id) VALUES (randomblob(16))",
            (),
        )
        .await?;
    }

    // Sync state table (for persisting transport-specific state like CloudKit server tokens)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS crr_sync_state (
            key   TEXT PRIMARY KEY,
            value BLOB
        )",
        (),
    )
    .await?;

    // Clock tables
    for (table, _) in CRR_TABLES {
        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {table}__crr_clock (
                pk       TEXT NOT NULL,
                col_name TEXT NOT NULL,
                col_ver  INTEGER NOT NULL,
                db_ver   INTEGER NOT NULL,
                site_id  BLOB NOT NULL,
                seq      INTEGER NOT NULL,
                PRIMARY KEY (pk, col_name)
            )"
        );
        conn.execute(&sql, ()).await?;
    }
    Ok(())
}

/// Get this device's 16-byte site UUID. Creates one if it doesn't exist.
pub async fn site_id(conn: &Connection) -> Result<Vec<u8>, turso::Error> {
    let mut rows = conn
        .query("SELECT site_id FROM crr_site_id LIMIT 1", ())
        .await?;
    if let Some(row) = rows.next().await? {
        if let Ok(val) = row.get_value(0) {
            if let Some(blob) = val.as_blob() {
                return Ok(blob.to_vec());
            }
        }
    }
    // Should have been created in migration, but create if missing
    conn.execute(
        "INSERT OR IGNORE INTO crr_site_id (site_id) VALUES (randomblob(16))",
        (),
    )
    .await?;
    let mut rows = conn
        .query("SELECT site_id FROM crr_site_id LIMIT 1", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    let val = row.get_value(0)?;
    Ok(val.as_blob().cloned().unwrap_or_default())
}

/// Get a value from the sync state table.
pub async fn get_sync_state(conn: &Connection, key: &str) -> Option<Vec<u8>> {
    let result = conn
        .query(
            "SELECT value FROM crr_sync_state WHERE key = ?1",
            turso::params::Params::Positional(vec![Value::Text(key.to_string())]),
        )
        .await;
    match result {
        Ok(mut rows) => {
            if let Ok(Some(row)) = rows.next().await {
                row.get_value(0)
                    .ok()
                    .and_then(|v| v.as_blob().cloned())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

/// Set a value in the sync state table.
pub async fn set_sync_state(
    conn: &Connection,
    key: &str,
    value: &[u8],
) -> Result<(), turso::Error> {
    conn.execute(
        "INSERT OR REPLACE INTO crr_sync_state (key, value) VALUES (?1, ?2)",
        turso::params::Params::Positional(vec![
            Value::Text(key.to_string()),
            Value::Blob(value.to_vec()),
        ]),
    )
    .await?;
    Ok(())
}

/// Get and increment the global db_version counter.
pub async fn next_db_version(conn: &Connection) -> Result<i64, turso::Error> {
    conn.execute(
        "UPDATE crr_db_version SET version = version + 1",
        (),
    )
    .await?;
    let mut rows = conn
        .query("SELECT version FROM crr_db_version LIMIT 1", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    Ok(row.get_value(0)?.as_integer().copied().unwrap_or(1))
}

/// Get the current db_version without incrementing.
pub async fn current_db_version(conn: &Connection) -> Result<i64, turso::Error> {
    let mut rows = conn
        .query("SELECT version FROM crr_db_version LIMIT 1", ())
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0))
}

// ── Change tracking (called after each mutation) ────────────────

/// Record an INSERT — sets sentinel CL=1 and all columns at col_ver=1.
pub async fn track_insert(
    conn: &Connection,
    table: &str,
    pk: &str,
    columns: &[&str],
) -> Result<(), turso::Error> {
    let site = site_id(conn).await?;
    let db_ver = next_db_version(conn).await?;
    let clock_table = format!("{table}__crr_clock");

    // Sentinel: marks row as alive (CL=1, odd)
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
             VALUES (?1, '__sentinel', 1, ?2, ?3, 0)"
        ),
        turso::params::Params::Positional(vec![
            Value::Text(pk.to_string()),
            Value::Integer(db_ver),
            Value::Blob(site.clone()),
        ]),
    )
    .await?;

    // One clock entry per column
    for (i, col) in columns.iter().enumerate() {
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
                 VALUES (?1, ?2, 1, ?3, ?4, ?5)"
            ),
            turso::params::Params::Positional(vec![
                Value::Text(pk.to_string()),
                Value::Text(col.to_string()),
                Value::Integer(db_ver),
                Value::Blob(site.clone()),
                Value::Integer(i as i64 + 1),
            ]),
        )
        .await?;
    }

    Ok(())
}

/// Record an UPDATE — increments col_ver for each changed column.
pub async fn track_update(
    conn: &Connection,
    table: &str,
    pk: &str,
    changed_columns: &[&str],
) -> Result<(), turso::Error> {
    let site = site_id(conn).await?;
    let db_ver = next_db_version(conn).await?;
    let clock_table = format!("{table}__crr_clock");

    for (i, col) in changed_columns.iter().enumerate() {
        // Get current col_ver and increment
        let current_ver = get_col_ver(conn, &clock_table, pk, col).await;
        conn.execute(
            &format!(
                "INSERT OR REPLACE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
            ),
            turso::params::Params::Positional(vec![
                Value::Text(pk.to_string()),
                Value::Text(col.to_string()),
                Value::Integer(current_ver + 1),
                Value::Integer(db_ver),
                Value::Blob(site.clone()),
                Value::Integer(i as i64),
            ]),
        )
        .await?;
    }

    Ok(())
}

/// Record a DELETE — increments sentinel CL to next even number.
pub async fn track_delete(
    conn: &Connection,
    table: &str,
    pk: &str,
) -> Result<(), turso::Error> {
    let site = site_id(conn).await?;
    let db_ver = next_db_version(conn).await?;
    let clock_table = format!("{table}__crr_clock");

    let current_cl = get_col_ver(conn, &clock_table, pk, "__sentinel").await;
    // Next even number = deleted state
    let new_cl = if current_cl % 2 == 1 {
        current_cl + 1
    } else {
        current_cl + 2
    };

    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
             VALUES (?1, '__sentinel', ?2, ?3, ?4, 0)"
        ),
        turso::params::Params::Positional(vec![
            Value::Text(pk.to_string()),
            Value::Integer(new_cl),
            Value::Integer(db_ver),
            Value::Blob(site),
        ]),
    )
    .await?;

    // Drop column clocks (keep only sentinel)
    conn.execute(
        &format!("DELETE FROM {clock_table} WHERE pk = ?1 AND col_name != '__sentinel'"),
        turso::params::Params::Positional(vec![Value::Text(pk.to_string())]),
    )
    .await?;

    Ok(())
}

// ── Reading changes (for producing changesets) ──────────────────

/// Read all changes since a given db_version across all CRR tables.
pub async fn changes_since(
    conn: &Connection,
    since_db_ver: i64,
) -> Result<Vec<ChangeRow>, turso::Error> {
    let mut all_changes = Vec::new();

    for (table, _columns) in CRR_TABLES {
        let clock_table = format!("{table}__crr_clock");

        // Get clock entries newer than since_db_ver
        let sql = format!(
            "SELECT pk, col_name, col_ver, db_ver, site_id, seq
             FROM {clock_table}
             WHERE db_ver > ?1
             ORDER BY db_ver, seq"
        );
        let mut rows = conn
            .query(&sql, [Value::Integer(since_db_ver)])
            .await?;

        while let Some(row) = rows.next().await? {
            let pk = row
                .get_value(0)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_default();
            let col_name = row
                .get_value(1)
                .ok()
                .and_then(|v| v.as_text().cloned())
                .unwrap_or_default();
            let col_ver = row
                .get_value(2)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0);
            let db_ver = row
                .get_value(3)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0);
            let site_id_blob = row
                .get_value(4)
                .ok()
                .and_then(|v| v.as_blob().map(|b| b.to_vec()))
                .unwrap_or_default();
            let seq = row
                .get_value(5)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0);

            // Get the sentinel CL for this row
            let cl = get_col_ver(conn, &clock_table, &pk, "__sentinel").await;

            // Get the actual column value from the data table (if not sentinel)
            let col_val = if col_name == "__sentinel" {
                serde_json::Value::Null
            } else {
                read_column_value(conn, table, &pk, &col_name).await
            };

            all_changes.push(ChangeRow {
                table_name: table.to_string(),
                pk,
                col_name,
                col_val,
                col_ver,
                db_ver,
                site_id: site_id_blob,
                seq,
                cl,
            });
        }
    }

    Ok(all_changes)
}

// ── Applying remote changes (merge) ─────────────────────────────

/// Apply a set of remote changes with LWW merge semantics.
pub async fn apply_changes(
    conn: &Connection,
    changes: &[ChangeRow],
) -> Result<MergeResult, turso::Error> {
    let mut result = MergeResult::default();
    let local_site = site_id(conn).await?;

    for change in changes {
        let clock_table = format!("{}__crr_clock", change.table_name);

        if change.col_name == "__sentinel" {
            // Handle row existence (insert/delete)
            let local_cl =
                get_col_ver(conn, &clock_table, &change.pk, "__sentinel").await;

            if change.cl < local_cl {
                result.skipped += 1;
                continue;
            }
            if change.cl == local_cl {
                result.skipped += 1;
                continue;
            }

            // Remote CL > local CL — apply
            let is_delete = change.cl % 2 == 0;
            let is_create = !is_delete && local_cl == 0;

            if is_create {
                // Row doesn't exist locally — create a skeleton row.
                // Column values will be filled by subsequent column-level changes.
                // We must supply defaults for NOT NULL columns.
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
            } else if is_delete {
                // Delete the row from the data table
                let sql = format!(
                    "DELETE FROM {} WHERE id = ?1",
                    change.table_name
                );
                let _ = conn
                    .execute(
                        &sql,
                        turso::params::Params::Positional(vec![Value::Text(
                            change.pk.clone(),
                        )]),
                    )
                    .await;
                // Drop all column clocks
                let _ = conn
                    .execute(
                        &format!(
                            "DELETE FROM {clock_table} WHERE pk = ?1 AND col_name != '__sentinel'"
                        ),
                        turso::params::Params::Positional(vec![Value::Text(
                            change.pk.clone(),
                        )]),
                    )
                    .await;
            }

            // Update sentinel clock
            let db_ver = next_db_version(conn).await?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
                     VALUES (?1, '__sentinel', ?2, ?3, ?4, 0)"
                ),
                turso::params::Params::Positional(vec![
                    Value::Text(change.pk.clone()),
                    Value::Integer(change.cl),
                    Value::Integer(db_ver),
                    Value::Blob(change.site_id.clone()),
                ]),
            )
            .await?;

            result.applied += 1;
        } else {
            // Column-level change — LWW merge
            let (local_ver, local_clock_site) =
                get_clock_entry(conn, &clock_table, &change.pk, &change.col_name).await;

            let wins = if change.col_ver > local_ver {
                true
            } else if change.col_ver < local_ver {
                false
            } else {
                // Tie-break: compare values, then site_id of the clock entry
                // (not the local device's site_id — the clock tracks who wrote last)
                let local_val = read_column_value(
                    conn,
                    &change.table_name,
                    &change.pk,
                    &change.col_name,
                )
                .await;
                let val_cmp = compare_json_values(&change.col_val, &local_val);
                if val_cmp != std::cmp::Ordering::Equal {
                    val_cmp == std::cmp::Ordering::Greater
                } else {
                    // Same value, same version — compare site_ids for final tie-break
                    // If same site_id wrote this, it's a duplicate — skip
                    change.site_id != local_clock_site && change.site_id > local_clock_site
                }
            };

            if !wins {
                result.skipped += 1;
                continue;
            }

            // Apply the winning value to the data table
            let sql = format!(
                "UPDATE {} SET {} = ?1 WHERE id = ?2",
                change.table_name, change.col_name
            );
            let val = json_to_turso_value(&change.col_val);
            let _ = conn
                .execute(
                    &sql,
                    turso::params::Params::Positional(vec![
                        val,
                        Value::Text(change.pk.clone()),
                    ]),
                )
                .await;

            // Update the clock
            let db_ver = next_db_version(conn).await?;
            conn.execute(
                &format!(
                    "INSERT OR REPLACE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
                ),
                turso::params::Params::Positional(vec![
                    Value::Text(change.pk.clone()),
                    Value::Text(change.col_name.clone()),
                    Value::Integer(change.col_ver),
                    Value::Integer(db_ver),
                    Value::Blob(change.site_id.clone()),
                    Value::Integer(change.seq),
                ]),
            )
            .await?;

            result.applied += 1;
        }
    }

    Ok(result)
}

// ── Internal helpers ────────────────────────────────────────────

/// Get (col_ver, site_id) for a specific (pk, col_name) in a clock table.
/// Returns (0, empty) if not found.
async fn get_clock_entry(
    conn: &Connection,
    clock_table: &str,
    pk: &str,
    col_name: &str,
) -> (i64, Vec<u8>) {
    let sql = format!(
        "SELECT col_ver, site_id FROM {clock_table} WHERE pk = ?1 AND col_name = ?2"
    );
    let result = conn
        .query(
            &sql,
            turso::params::Params::Positional(vec![
                Value::Text(pk.to_string()),
                Value::Text(col_name.to_string()),
            ]),
        )
        .await;
    match result {
        Ok(mut rows) => {
            if let Ok(Some(row)) = rows.next().await {
                let ver = row
                    .get_value(0)
                    .ok()
                    .and_then(|v| v.as_integer().copied())
                    .unwrap_or(0);
                let site = row
                    .get_value(1)
                    .ok()
                    .and_then(|v| v.as_blob().cloned())
                    .unwrap_or_default();
                (ver, site)
            } else {
                (0, Vec::new())
            }
        }
        Err(_) => (0, Vec::new()),
    }
}

/// Get the col_ver for a specific (pk, col_name) in a clock table. Returns 0 if not found.
async fn get_col_ver(conn: &Connection, clock_table: &str, pk: &str, col_name: &str) -> i64 {
    let sql = format!(
        "SELECT col_ver FROM {clock_table} WHERE pk = ?1 AND col_name = ?2"
    );
    let result = conn
        .query(
            &sql,
            turso::params::Params::Positional(vec![
                Value::Text(pk.to_string()),
                Value::Text(col_name.to_string()),
            ]),
        )
        .await;
    match result {
        Ok(mut rows) => {
            if let Ok(Some(row)) = rows.next().await {
                row.get_value(0)
                    .ok()
                    .and_then(|v| v.as_integer().copied())
                    .unwrap_or(0)
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

/// Read a single column value from a data table row.
async fn read_column_value(
    conn: &Connection,
    table: &str,
    pk: &str,
    col_name: &str,
) -> serde_json::Value {
    let sql = format!("SELECT {col_name} FROM {table} WHERE id = ?1");
    let result = conn
        .query(&sql, turso::params::Params::Positional(vec![Value::Text(pk.to_string())]))
        .await;
    match result {
        Ok(mut rows) => {
            if let Ok(Some(row)) = rows.next().await {
                turso_value_to_json(row.get_value(0).ok().as_ref())
            } else {
                serde_json::Value::Null
            }
        }
        Err(_) => serde_json::Value::Null,
    }
}

/// Convert a turso Value to serde_json::Value.
fn turso_value_to_json(val: Option<&turso::Value>) -> serde_json::Value {
    match val {
        Some(turso::Value::Text(s)) => serde_json::Value::String(s.clone()),
        Some(turso::Value::Integer(i)) => serde_json::Value::Number((*i).into()),
        Some(turso::Value::Real(f)) => {
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Some(turso::Value::Null) | None => serde_json::Value::Null,
        Some(turso::Value::Blob(b)) => {
            serde_json::Value::String(base64_encode(b))
        }
    }
}

/// Convert a serde_json::Value back to a turso Value.
fn json_to_turso_value(val: &serde_json::Value) -> Value {
    match val {
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Real(f)
            } else {
                Value::Null
            }
        }
        serde_json::Value::Bool(b) => Value::Integer(*b as i64),
        serde_json::Value::Null => Value::Null,
        _ => Value::Text(val.to_string()),
    }
}

/// Deterministic comparison of JSON values for tie-breaking.
fn compare_json_values(a: &serde_json::Value, b: &serde_json::Value) -> std::cmp::Ordering {
    let a_str = a.to_string();
    let b_str = b.to_string();
    a_str.cmp(&b_str)
}

/// Create a skeleton row in a data table with defaults for NOT NULL columns.
/// This is needed when applying a remote INSERT — we create the row first,
/// then column-level changes fill in the actual values.
async fn create_skeleton_row(conn: &Connection, table: &str, pk: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let sql = match table {
        "papers" => format!(
            "INSERT OR IGNORE INTO papers (id, title, authors, date_added, date_modified, is_favorite, is_read) \
             VALUES (?1, '', '[]', '{now}', '{now}', 0, 0)"
        ),
        "collections" => format!(
            "INSERT OR IGNORE INTO collections (id, name, position) VALUES (?1, '', 0)"
        ),
        "tags" => format!(
            "INSERT OR IGNORE INTO tags (id, name) VALUES (?1, '')"
        ),
        "annotations" => format!(
            "INSERT OR IGNORE INTO annotations (id, paper_id, page, ann_type, color, geometry, created_at, modified_at) \
             VALUES (?1, '', 0, 'note', '#ffff00', '{{}}', '{now}', '{now}')"
        ),
        "notes" => format!(
            "INSERT OR IGNORE INTO notes (id, paper_id, title, body, created_at, modified_at) \
             VALUES (?1, '', '', '', '{now}', '{now}')"
        ),
        "saved_searches" => format!(
            "INSERT OR IGNORE INTO saved_searches (id, name, query, created_at) \
             VALUES (?1, '', '', '{now}')"
        ),
        "paper_collections" => {
            // Junction table — pk is "paper_id:collection_id"
            let parts: Vec<&str> = pk.splitn(2, ':').collect();
            if parts.len() == 2 {
                let sql = "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) VALUES (?1, ?2)";
                let _ = conn.execute(
                    sql,
                    turso::params::Params::Positional(vec![
                        Value::Text(parts[0].to_string()),
                        Value::Text(parts[1].to_string()),
                    ]),
                ).await;
            }
            return;
        }
        "paper_tags" => {
            let parts: Vec<&str> = pk.splitn(2, ':').collect();
            if parts.len() == 2 {
                let sql = "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)";
                let _ = conn.execute(
                    sql,
                    turso::params::Params::Positional(vec![
                        Value::Text(parts[0].to_string()),
                        Value::Text(parts[1].to_string()),
                    ]),
                ).await;
            }
            return;
        }
        _ => return,
    };
    let _ = conn.execute(
        &sql,
        turso::params::Params::Positional(vec![Value::Text(pk.to_string())]),
    ).await;
}

/// Simple base64 encoding for blob values.
fn base64_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for byte in bytes {
        write!(s, "{:02x}", byte).unwrap();
    }
    s
}

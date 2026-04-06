//! Internal helper functions shared across CRR submodules.

use turso::{Connection, Value};

/// Get (col_ver, site_id) for a specific (pk, col_name) in a clock table.
/// Returns (0, empty) if not found.
pub(crate) async fn get_clock_entry(
    conn: &Connection,
    clock_table: &str,
    pk: &str,
    col_name: &str,
) -> (i64, Vec<u8>) {
    let sql = format!("SELECT col_ver, site_id FROM {clock_table} WHERE pk = ?1 AND col_name = ?2");
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
pub(crate) async fn get_col_ver(
    conn: &Connection,
    clock_table: &str,
    pk: &str,
    col_name: &str,
) -> i64 {
    let sql = format!("SELECT col_ver FROM {clock_table} WHERE pk = ?1 AND col_name = ?2");
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
pub(crate) async fn read_column_value(
    conn: &Connection,
    table: &str,
    pk: &str,
    col_name: &str,
) -> serde_json::Value {
    let sql = format!("SELECT {col_name} FROM {table} WHERE id = ?1");
    let result = conn
        .query(
            &sql,
            turso::params::Params::Positional(vec![Value::Text(pk.to_string())]),
        )
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
pub(crate) fn turso_value_to_json(val: Option<&turso::Value>) -> serde_json::Value {
    match val {
        Some(turso::Value::Text(s)) => serde_json::Value::String(s.clone()),
        Some(turso::Value::Integer(i)) => serde_json::Value::Number((*i).into()),
        Some(turso::Value::Real(f)) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(turso::Value::Null) | None => serde_json::Value::Null,
        Some(turso::Value::Blob(b)) => serde_json::Value::String(base64_encode(b)),
    }
}

/// Convert a serde_json::Value back to a turso Value.
pub(crate) fn json_to_turso_value(val: &serde_json::Value) -> Value {
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
pub(crate) fn compare_json_values(
    a: &serde_json::Value,
    b: &serde_json::Value,
) -> std::cmp::Ordering {
    let a_str = a.to_string();
    let b_str = b.to_string();
    a_str.cmp(&b_str)
}

/// Zero all non-sentinel column clocks for a row. Used during resurrection
/// so that any incoming column values (col_ver >= 1) automatically win.
pub(crate) async fn zero_column_clocks(conn: &Connection, clock_table: &str, pk: &str) {
    let sql =
        format!("UPDATE {clock_table} SET col_ver = 0 WHERE pk = ?1 AND col_name != '__sentinel'");
    let _ = conn
        .execute(
            &sql,
            turso::params::Params::Positional(vec![Value::Text(pk.to_string())]),
        )
        .await;
}

/// Create a skeleton row in a data table with defaults for NOT NULL columns.
/// This is needed when applying a remote INSERT — we create the row first,
/// then column-level changes fill in the actual values.
pub(crate) async fn create_skeleton_row(conn: &Connection, table: &str, pk: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    let sql = match table {
        "papers" => format!(
            "INSERT OR IGNORE INTO papers (id, title, authors, date_added, date_modified, is_favorite, is_read) \
             VALUES (?1, '', '[]', '{now}', '{now}', 0, 0)"
        ),
        "collections" => {
            format!("INSERT OR IGNORE INTO collections (id, name, position) VALUES (?1, '', 0)")
        }
        "tags" => format!("INSERT OR IGNORE INTO tags (id, name) VALUES (?1, '')"),
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
                let _ = conn
                    .execute(
                        sql,
                        turso::params::Params::Positional(vec![
                            Value::Text(parts[0].to_string()),
                            Value::Text(parts[1].to_string()),
                        ]),
                    )
                    .await;
            }
            return;
        }
        "paper_tags" => {
            let parts: Vec<&str> = pk.splitn(2, ':').collect();
            if parts.len() == 2 {
                let sql = "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)";
                let _ = conn
                    .execute(
                        sql,
                        turso::params::Params::Positional(vec![
                            Value::Text(parts[0].to_string()),
                            Value::Text(parts[1].to_string()),
                        ]),
                    )
                    .await;
            }
            return;
        }
        _ => return,
    };
    let _ = conn
        .execute(
            &sql,
            turso::params::Params::Positional(vec![Value::Text(pk.to_string())]),
        )
        .await;
}

/// Re-export site_id from state module for internal use.
pub(crate) use super::state::site_id;

/// Simple base64 encoding for blob values.
fn base64_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 4 / 3 + 4);
    for byte in bytes {
        write!(s, "{:02x}", byte).unwrap();
    }
    s
}

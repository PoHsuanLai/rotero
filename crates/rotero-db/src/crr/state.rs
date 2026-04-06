//! Sync state persistence — site ID, db version, and sync state key-value store.

use turso::{Connection, Value};

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

/// Get and increment the global db_version counter.
pub async fn next_db_version(conn: &Connection) -> Result<i64, turso::Error> {
    conn.execute("UPDATE crr_db_version SET version = version + 1", ())
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
                row.get_value(0).ok().and_then(|v| v.as_blob().cloned())
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

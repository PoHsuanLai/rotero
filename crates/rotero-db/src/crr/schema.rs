//! CRR table initialization.

use turso::Connection;

use super::CRR_TABLES;

/// Create CRR metadata and clock tables (idempotent).
pub async fn init_crr_tables(conn: &Connection) -> Result<(), turso::Error> {
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
    let mut rows = conn
        .query("SELECT version FROM crr_db_version LIMIT 1", ())
        .await?;
    if rows.next().await?.is_none() {
        conn.execute("INSERT INTO crr_db_version (version) VALUES (0)", ())
            .await?;
    }
    let mut rows = conn
        .query("SELECT site_id FROM crr_site_id LIMIT 1", ())
        .await?;
    if rows.next().await?.is_none() {
        conn.execute(
            "INSERT INTO crr_site_id (site_id) VALUES (randomblob(16))",
            (),
        )
        .await?;
    }

    // Transport-specific state (e.g. CloudKit server tokens)
    conn.execute(
        "CREATE TABLE IF NOT EXISTS crr_sync_state (
            key   TEXT PRIMARY KEY,
            value BLOB
        )",
        (),
    )
    .await?;

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

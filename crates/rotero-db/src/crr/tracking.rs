//! Change tracking: called after each mutation to record changes for sync.

use turso::{Connection, Value};

use super::helpers::{get_col_ver, site_id};
use super::next_db_version;

/// Record an INSERT: sentinel CL=1, all columns at col_ver=1.
pub async fn track_insert(
    conn: &Connection,
    table: &str,
    pk: &str,
    columns: &[&str],
) -> Result<(), turso::Error> {
    let site = site_id(conn).await?;
    let db_ver = next_db_version(conn).await?;
    let clock_table = format!("{table}__crr_clock");

    // Sentinel marks row as alive (CL=1, odd)
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

/// Record an UPDATE: increments col_ver for each changed column.
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

/// Record a DELETE: increments sentinel CL to next even number.
pub async fn track_delete(conn: &Connection, table: &str, pk: &str) -> Result<(), turso::Error> {
    let site = site_id(conn).await?;
    let db_ver = next_db_version(conn).await?;
    let clock_table = format!("{table}__crr_clock");

    let current_cl = get_col_ver(conn, &clock_table, pk, "__sentinel").await;
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

    // Note: column clocks are intentionally preserved (not dropped) so that
    // resurrection can zero them and let incoming values win.

    Ok(())
}

/// Read all changes since a given db_version.
pub async fn changes_since(
    conn: &Connection,
    since_db_ver: i64,
) -> Result<Vec<super::ChangeRow>, turso::Error> {
    let mut all_changes = Vec::new();

    for (table, _columns) in super::CRR_TABLES {
        let clock_table = format!("{table}__crr_clock");

        let sql = format!(
            "SELECT pk, col_name, col_ver, db_ver, site_id, seq
             FROM {clock_table}
             WHERE db_ver > ?1
             ORDER BY db_ver, seq"
        );
        let mut rows = conn.query(&sql, [Value::Integer(since_db_ver)]).await?;

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

            let cl = get_col_ver(conn, &clock_table, &pk, "__sentinel").await;

            let col_val = if col_name == "__sentinel" {
                serde_json::Value::Null
            } else {
                super::helpers::read_column_value(conn, table, &pk, &col_name).await
            };

            all_changes.push(super::ChangeRow {
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

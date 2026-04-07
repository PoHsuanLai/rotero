//! LWW merge logic for applying remote changes.

use turso::{Connection, Value};

use super::helpers::{
    compare_json_values, create_skeleton_row, get_clock_entry, get_col_ver, json_to_turso_value,
    read_column_value, site_id, zero_column_clocks,
};
use super::{ChangeRow, MergeResult};
use super::next_db_version;

/// Apply remote changes with LWW merge semantics.
pub async fn apply_changes(
    conn: &Connection,
    changes: &[ChangeRow],
) -> Result<MergeResult, turso::Error> {
    let mut result = MergeResult::default();
    let _local_site = site_id(conn).await?;

    for change in changes {
        let clock_table = format!("{}__crr_clock", change.table_name);

        if change.col_name == "__sentinel" {
            let local_cl = get_col_ver(conn, &clock_table, &change.pk, "__sentinel").await;

            if change.cl < local_cl {
                result.skipped += 1;
                continue;
            }
            if change.cl == local_cl {
                result.skipped += 1;
                continue;
            }

            let is_delete = change.cl % 2 == 0;
            let is_create = !is_delete && local_cl == 0;

            let is_resurrect = !is_delete && local_cl > 0 && local_cl % 2 == 0;

            if is_create {
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
            } else if is_resurrect {
                // Resurrect: re-create row and zero clocks so incoming values win
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
                zero_column_clocks(conn, &clock_table, &change.pk).await;
            } else if is_delete {
                let sql = format!("DELETE FROM {} WHERE id = ?1", change.table_name);
                let _ = conn
                    .execute(
                        &sql,
                        turso::params::Params::Positional(vec![Value::Text(change.pk.clone())]),
                    )
                    .await;
            }

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
            // Column-level LWW merge
            let local_sentinel_cl = get_col_ver(conn, &clock_table, &change.pk, "__sentinel").await;

            if local_sentinel_cl == 0 {
                // Out-of-order: column arrived before sentinel
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
                let db_ver = next_db_version(conn).await?;
                conn.execute(
                    &format!(
                        "INSERT OR IGNORE INTO {clock_table} (pk, col_name, col_ver, db_ver, site_id, seq)
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
            } else if local_sentinel_cl % 2 == 0
                && change.cl % 2 == 1
                && change.cl > local_sentinel_cl
            {
                // Resurrect: column from newer alive state than local delete
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
                zero_column_clocks(conn, &clock_table, &change.pk).await;
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
            } else if local_sentinel_cl % 2 == 0 {
                result.skipped += 1;
                continue;
            }

            let (local_ver, local_clock_site) =
                get_clock_entry(conn, &clock_table, &change.pk, &change.col_name).await;

            let wins = if change.col_ver > local_ver {
                true
            } else if change.col_ver < local_ver {
                false
            } else {
                // Tie-break: compare values, then site_id (of clock entry, not local device)
                let local_val =
                    read_column_value(conn, &change.table_name, &change.pk, &change.col_name).await;
                let val_cmp = compare_json_values(&change.col_val, &local_val);
                if val_cmp != std::cmp::Ordering::Equal {
                    val_cmp == std::cmp::Ordering::Greater
                } else {
                    // Final tie-break by site_id; same site means duplicate
                    change.site_id != local_clock_site && change.site_id > local_clock_site
                }
            };

            if !wins {
                result.skipped += 1;
                continue;
            }

            let sql = format!(
                "UPDATE {} SET {} = ?1 WHERE id = ?2",
                change.table_name, change.col_name
            );
            let val = json_to_turso_value(&change.col_val);
            let _ = conn
                .execute(
                    &sql,
                    turso::params::Params::Positional(vec![val, Value::Text(change.pk.clone())]),
                )
                .await;

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

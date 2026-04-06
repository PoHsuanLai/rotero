//! Merge logic and conflict resolution for applying remote changes.

use turso::{Connection, Value};

use super::helpers::{
    compare_json_values, create_skeleton_row, get_clock_entry, get_col_ver, json_to_turso_value,
    read_column_value, site_id, zero_column_clocks,
};
use super::{ChangeRow, MergeResult};
use super::next_db_version;

/// Apply a set of remote changes with LWW merge semantics.
pub async fn apply_changes(
    conn: &Connection,
    changes: &[ChangeRow],
) -> Result<MergeResult, turso::Error> {
    let mut result = MergeResult::default();
    let _local_site = site_id(conn).await?;

    for change in changes {
        let clock_table = format!("{}__crr_clock", change.table_name);

        if change.col_name == "__sentinel" {
            // Handle row existence (insert/delete)
            let local_cl = get_col_ver(conn, &clock_table, &change.pk, "__sentinel").await;

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

            let is_resurrect = !is_delete && local_cl > 0 && local_cl % 2 == 0;

            if is_create {
                // Row doesn't exist locally — create a skeleton row.
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
            } else if is_resurrect {
                // Row was deleted locally, remote says it's alive with higher CL — resurrect.
                // Re-create the data row and zero all column clocks so incoming values win.
                create_skeleton_row(conn, &change.table_name, &change.pk).await;
                zero_column_clocks(conn, &clock_table, &change.pk).await;
            } else if is_delete {
                // Delete the row from the data table (but preserve column clocks
                // so resurrection can zero them later).
                let sql = format!("DELETE FROM {} WHERE id = ?1", change.table_name);
                let _ = conn
                    .execute(
                        &sql,
                        turso::params::Params::Positional(vec![Value::Text(change.pk.clone())]),
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

            // First, ensure the row exists. It may be missing (out-of-order) or deleted.
            let local_sentinel_cl = get_col_ver(conn, &clock_table, &change.pk, "__sentinel").await;

            if local_sentinel_cl == 0 {
                // Row doesn't exist — column change arrived before sentinel (out-of-order).
                // Create skeleton and a provisional sentinel.
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
                // Row deleted locally, but this column change is from a newer alive state — resurrect.
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
                // Row is deleted and column change doesn't warrant resurrection — skip.
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
                // Tie-break: compare values, then site_id of the clock entry
                // (not the local device's site_id — the clock tracks who wrote last)
                let local_val =
                    read_column_value(conn, &change.table_name, &change.pk, &change.col_name).await;
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
                    turso::params::Params::Positional(vec![val, Value::Text(change.pk.clone())]),
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

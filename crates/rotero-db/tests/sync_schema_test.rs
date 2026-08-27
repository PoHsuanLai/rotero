//! `SYNCED_TABLES` must describe the database it claims to describe.
//!
//! The manifest decides what a snapshot carries and what the merge applies. A
//! column in the database but missing from the manifest is silently never
//! synced — the row saves locally, looks right in the UI, and never reaches the
//! other device. That is the failure mode this whole rewrite exists to remove,
//! so it is checked mechanically rather than by review.

mod common;

use rotero_db::sync_schema::{SYNC_COLUMNS, SYNCED_TABLES};

/// Columns a table has in SQLite, via `PRAGMA table_info`.
async fn actual_columns(db: &rotero_db::Database, table: &str) -> Vec<String> {
    let mut rows = db
        .conn()
        .query(&format!("PRAGMA table_info({table})"), ())
        .await
        .unwrap();
    let mut cols = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        if let Some(name) = row.get_value(1).ok().and_then(|v| v.as_text().cloned()) {
            cols.push(name);
        }
    }
    cols
}

/// Every column the manifest names must exist in the table.
#[tokio::test]
async fn every_manifest_column_exists() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    for table in SYNCED_TABLES {
        let actual = actual_columns(&db, table.name).await;
        assert!(
            !actual.is_empty(),
            "synced table `{}` does not exist",
            table.name
        );
        for column in table.all_columns() {
            assert!(
                actual.iter().any(|c| c == column),
                "`{}` names column `{column}`, which the table does not have",
                table.name
            );
        }
    }
}

/// Every column the table has must be accounted for by the manifest.
///
/// The reverse direction, and the one that actually catches drift: adding a
/// column to the SQL and forgetting the manifest means it never syncs. Anything
/// deliberately local-only belongs in `LOCAL_ONLY` with a reason, so the choice
/// is visible rather than an omission.
#[tokio::test]
async fn every_table_column_is_accounted_for() {
    /// Columns that exist but deliberately do not sync.
    const LOCAL_ONLY: &[(&str, &str)] = &[
        // Re-extractable from the PDF, dominates the table's size, and syncing
        // it would let a background extraction overwrite a real metadata edit.
        ("papers", "fulltext"),
        // Derived on demand for citation export.
        ("papers", "citation_key"),
        // Resolved per-device from the local OA download path.
        ("papers", "pdf_url"),
    ];

    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    for table in SYNCED_TABLES {
        let declared = table.all_columns();
        for actual in actual_columns(&db, table.name).await {
            let known = declared.iter().any(|c| *c == actual)
                || SYNC_COLUMNS.contains(&actual.as_str())
                || LOCAL_ONLY
                    .iter()
                    .any(|(t, c)| *t == table.name && *c == actual);
            assert!(
                known,
                "`{}`.`{actual}` is in the database but not in SYNCED_TABLES. \
                 Add it to the manifest so it syncs, or to LOCAL_ONLY with a reason.",
                table.name
            );
        }
    }
}

/// Composite-key tables must declare both key columns.
#[tokio::test]
async fn composite_keys_match_the_schema() {
    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    for table in SYNCED_TABLES {
        let mut rows = db
            .conn()
            .query(&format!("PRAGMA table_info({})", table.name), ())
            .await
            .unwrap();
        let mut pk_cols = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            let name = row.get_value(1).ok().and_then(|v| v.as_text().cloned());
            let pk_pos = row.get_value(5).ok().and_then(|v| v.as_integer().copied());
            if let (Some(name), Some(pos)) = (name, pk_pos)
                && pos > 0
            {
                pk_cols.push((pos, name));
            }
        }
        // Compared as sets: turso's `table_info` reports composite key columns
        // in its own order, not the order the DDL declares them, so requiring a
        // matching sequence would fail on a schema that is in fact correct. The
        // membership is what matters — it is what identifies a snapshot row.
        let mut actual: Vec<String> = pk_cols.into_iter().map(|(_, n)| n).collect();
        let mut declared: Vec<String> = table.pk.columns().iter().map(|s| s.to_string()).collect();
        actual.sort();
        declared.sort();
        assert_eq!(
            actual, declared,
            "`{}` declares primary key {declared:?} but the table has {actual:?}",
            table.name
        );
    }
}

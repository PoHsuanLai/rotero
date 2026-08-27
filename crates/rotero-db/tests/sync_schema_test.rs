//! `SYNCED_TABLES` must describe the database it claims to describe.
//!
//! The manifest decides what a snapshot carries and what the merge applies. A
//! column — or a whole table — in the database but missing from the manifest is
//! silently never synced: the row saves locally, looks right in the UI, and
//! never reaches the other device. That is the failure mode this whole rewrite
//! exists to remove, so it is checked mechanically rather than by review.
//!
//! Both directions are checked, and in both the allowlist is the point: staying
//! local is a legitimate choice, but it has to be written down rather than
//! arrived at by forgetting.

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

/// Every table in the database must either sync or say why it does not.
///
/// The column checks above only ever look at tables the manifest already names,
/// so a whole table added to the SQL and forgotten is invisible to them — the
/// one direction that was still uncovered. It is also the easier mistake: a new
/// feature adds its table, the app works on that device, and nothing says the
/// data never leaves it.
///
/// Being local-only is a legitimate answer, and often the right one. The point
/// is that it has to be *chosen* — an entry here — rather than reached by
/// forgetting the manifest exists.
#[tokio::test]
async fn every_table_either_syncs_or_is_declared_local() {
    /// Tables that exist but deliberately do not sync, and why.
    const LOCAL_ONLY_TABLES: &[(&str, &str)] = &[
        // Records what THIS install has done, not shared library state.
        ("app_flags", "per-install task bookkeeping"),
        // This device's sync identity. Replicating it would give two devices
        // the same name, and every clock comparison keys on that name.
        ("crr_site_id", "this device's own identity"),
        // Where the local schema has got to. A peer mid-migration would
        // otherwise be told it is already done.
        ("schema_version", "local migration state"),
        // Session ids are minted by the agent binary on this machine, so a
        // synced row would name a session that resolves to nothing elsewhere.
        // These carry none of the bookkeeping columns and have no `_live` view.
        ("chat_sessions", "agent session ids are machine-local"),
        ("chat_session_papers", "child of chat_sessions"),
    ];

    let dir = tempfile::tempdir().unwrap();
    let db = common::open_test_db(dir.path()).await;

    let mut rows = db
        .conn()
        .query(
            "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
            (),
        )
        .await
        .unwrap();

    while let Some(row) = rows.next().await.unwrap() {
        let Some(name) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) else {
            continue;
        };
        // SQLite's own bookkeeping and turso's FTS shadow tables are not ours
        // to classify: they are created by the engine, and their names are an
        // implementation detail rather than a decision anyone makes.
        if name.starts_with("sqlite_") || name.starts_with("__turso_internal_") {
            continue;
        }
        let synced = SYNCED_TABLES.iter().any(|t| t.name == name);
        let local = LOCAL_ONLY_TABLES.iter().any(|(t, _)| *t == name);
        assert!(
            synced || local,
            "table `{name}` is in the database but neither in SYNCED_TABLES nor \
             LOCAL_ONLY_TABLES. Add it to the manifest so it syncs, or to \
             LOCAL_ONLY_TABLES with the reason it must not."
        );
        assert!(
            !(synced && local),
            "table `{name}` is both in SYNCED_TABLES and LOCAL_ONLY_TABLES"
        );
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

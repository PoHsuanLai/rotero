//! Schema v21 adds the wiki tables.
//!
//! A library recorded at v20 has no concepts, claims, edges, or stubs. Opening
//! it with this build creates them, their live views, and their sync columns.

mod common;

use rotero_db::sync_schema::SYNCED_TABLES;

async fn rewind_to_v20(dir: &std::path::Path) {
    let db_path = dir.join("rotero.db");
    let raw = turso::Builder::new_local(db_path.to_str().unwrap())
        .experimental_index_method(true)
        .build()
        .await
        .unwrap();
    let conn = raw.connect().unwrap();
    for statement in [
        "DROP VIEW IF EXISTS concepts_live",
        "DROP VIEW IF EXISTS claims_live",
        "DROP VIEW IF EXISTS wiki_edges_live",
        "DROP VIEW IF EXISTS reference_stubs_live",
        "DROP TABLE IF EXISTS concepts",
        "DROP TABLE IF EXISTS claims",
        "DROP TABLE IF EXISTS wiki_edges",
        "DROP TABLE IF EXISTS reference_stubs",
    ] {
        conn.execute(statement, ()).await.unwrap();
    }
    conn.execute(
        "UPDATE schema_version SET version = ?1",
        [turso::Value::Integer(20)],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn opening_a_v20_library_creates_the_wiki_tables() {
    let dir = tempfile::tempdir().unwrap();
    common::open_test_db(dir.path()).await;
    rewind_to_v20(dir.path()).await;

    let db = common::open_test_db(dir.path()).await;
    for name in ["concepts", "claims", "wiki_edges", "reference_stubs"] {
        let spec = SYNCED_TABLES.iter().find(|t| t.name == name).unwrap();
        let mut rows = db
            .conn()
            .query(&format!("PRAGMA table_info({name})"), ())
            .await
            .unwrap();
        let mut cols = Vec::new();
        while let Some(row) = rows.next().await.unwrap() {
            if let Some(col) = row.get_value(1).ok().and_then(|v| v.as_text().cloned()) {
                cols.push(col);
            }
        }
        for column in spec.all_columns() {
            assert!(
                cols.iter().any(|c| c == column),
                "{name} is missing {column} after migrating from v20"
            );
        }
        let mut views = db
            .conn()
            .query(
                "SELECT name FROM sqlite_master WHERE type = 'view' AND name = ?1",
                [turso::Value::Text(format!("{name}_live"))],
            )
            .await
            .unwrap();
        assert!(
            views.next().await.unwrap().is_some(),
            "{name}_live was not created"
        );
    }
}

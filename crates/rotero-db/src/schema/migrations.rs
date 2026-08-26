//! Schema migration logic.

use turso::Connection;

use super::tables::{CREATE_FTS_INDEX, CREATE_LIVE_VIEWS, CREATE_TABLES};

/// Current schema version; incremented with each migration.
pub const SCHEMA_VERSION: i64 = 14;

/// Why a database could not be prepared for use.
#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    /// A driver error from creating tables or running a migration.
    #[error(transparent)]
    Turso(#[from] turso::Error),
    /// The library was written by a newer build than this one.
    ///
    /// Distinct from a driver error because it is the user's to resolve, and
    /// because continuing would let older code write a schema it does not
    /// understand — the way a synced library gets corrupted.
    #[error(
        "This library was created by a newer version of Rotero \
         (library schema v{found}, this version supports v{supported}). \
         Update Rotero to open it."
    )]
    NewerSchema {
        /// The version recorded in the library.
        found: i64,
        /// The newest version this build understands.
        supported: i64,
    },
}

/// Create the application tables and run pending migrations.
///
/// CRR metadata tables are created separately by [`crate::Database::open`] via
/// the `recrr` store's `init()`.
pub async fn initialize_db(conn: &Connection) -> Result<(), SchemaError> {
    conn.execute_batch(CREATE_TABLES).await?;
    create_live_views(conn).await;

    run_migrations(conn).await?;

    Ok(())
}

/// Drop and recreate the FTS index so it is consistent with the current
/// `papers` rows. A stale index (from an older engine version or the
/// table-rebuild migration) makes `fts_score` return 0.0 and `fts_match` return
/// an unreliable row set; a fresh rebuild restores correct BM25 ranking. Called
/// once from the version-10 migration — turso maintains the index incrementally
/// on writes thereafter. Best-effort: search falls back to LIKE if this fails.
async fn rebuild_fts_index(conn: &Connection) {
    let _ = conn
        .execute("DROP INDEX IF EXISTS idx_papers_fts", ())
        .await;
    let _ = conn.execute(CREATE_FTS_INDEX, ()).await;
}

/// Rewrite each `papers.doi` to its canonical stored form.
///
/// Only rows whose canonical form differs from the stored value are touched.
/// Unparseable-but-present values are left unchanged (they canonicalize to
/// themselves). Best-effort: a failed row is skipped so one bad value can't
/// abort the migration.
async fn backfill_canonical_dois(conn: &Connection) {
    use rotero_models::PaperId;

    let mut updates: Vec<(String, String)> = Vec::new(); // (id, canonical_doi)
    let Ok(mut rows) = conn
        .query(
            "SELECT id, doi FROM papers WHERE doi IS NOT NULL AND doi != ''",
            (),
        )
        .await
    else {
        return;
    };
    while let Ok(Some(row)) = rows.next().await {
        let Some(id) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) else {
            continue;
        };
        let Some(doi) = row.get_value(1).ok().and_then(|v| v.as_text().cloned()) else {
            continue;
        };
        let canonical = match PaperId::parse(&doi) {
            Some(pid) => pid.to_stored_string(),
            None => continue, // keep unrecognized values verbatim
        };
        if canonical != doi {
            updates.push((id, canonical));
        }
    }

    for (id, canonical) in updates {
        let _ = conn
            .execute(
                "UPDATE papers SET doi = ?1 WHERE id = ?2",
                turso::params::Params::Positional(vec![
                    turso::Value::Text(canonical),
                    turso::Value::Text(id),
                ]),
            )
            .await;
    }
}

async fn run_migrations(conn: &Connection) -> Result<(), SchemaError> {
    let current_version = get_schema_version(conn).await?;

    // Refuse a library from a newer build rather than running its rows through
    // migrations that predate its schema. Every migration below is written
    // against columns this build knows about; a newer database may have renamed
    // or dropped them, and the writes would corrupt data that is also syncing to
    // the machine that created it.
    if current_version > SCHEMA_VERSION {
        return Err(SchemaError::NewerSchema {
            found: current_version,
            supported: SCHEMA_VERSION,
        });
    }

    if current_version < 1 {
        // Fresh database
        let _ = conn.execute(CREATE_FTS_INDEX, ()).await;
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )
        .await?;
        return Ok(());
    }

    if current_version < 2 {
        let _ = conn
            .execute(
                "ALTER TABLE papers ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0",
                (),
            )
            .await;
        let _ = conn
            .execute(
                "ALTER TABLE papers ADD COLUMN is_read INTEGER NOT NULL DEFAULT 0",
                (),
            )
            .await;
    }

    if current_version < 3 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN fulltext TEXT", ())
            .await;
    }

    if current_version < 4 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN citation_count INTEGER", ())
            .await;
        let _ = conn
            .execute(
                "CREATE TABLE IF NOT EXISTS saved_searches (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    name       TEXT NOT NULL,
                    query      TEXT NOT NULL,
                    created_at TEXT NOT NULL
                )",
                (),
            )
            .await;
    }

    if current_version < 5 {
        let _ = conn.execute(CREATE_FTS_INDEX, ()).await;
    }

    if current_version < 6 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN citation_key TEXT", ())
            .await;
    }

    if current_version < 7 {
        const PALETTE: &[&str] = &[
            "#6b7085", "#7c6b85", "#6b8580", "#857a6b", "#6b7a85", "#856b7a", "#6b856e", "#85706b",
            "#6e6b85", "#7a856b", "#856b6b", "#6b8585",
        ];
        let mut rows = conn.query(crate::queries::TAG_LIST_NULL_COLOR, ()).await?;
        let mut updates: Vec<(i64, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
            let name = row.get_value(1)?.as_text().cloned().unwrap_or_default();
            let hash = name
                .bytes()
                .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
            updates.push((id, PALETTE[hash % PALETTE.len()].to_string()));
        }
        for (id, color) in updates {
            let _ = conn
                .execute(
                    crate::queries::TAG_UPDATE_COLOR,
                    turso::params::Params::Positional(vec![
                        turso::Value::Text(color),
                        turso::Value::Integer(id),
                    ]),
                )
                .await;
        }
    }

    // Idempotent: ensure columns exist even if earlier migrations partially ran
    // (e.g. the version counter advanced but the ALTER didn't land). Each is a
    // no-op once the column is present.
    let _ = conn
        .execute("ALTER TABLE papers ADD COLUMN citation_count INTEGER", ())
        .await;

    let _ = conn
        .execute("ALTER TABLE papers ADD COLUMN pdf_url TEXT", ())
        .await;

    let _ = conn
        .execute(
            "ALTER TABLE papers ADD COLUMN item_type TEXT NOT NULL DEFAULT 'journalArticle'",
            (),
        )
        .await;

    if current_version < 8 {
        migrate_to_text_ids(conn).await?;
    }

    if current_version < 9 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN pdf_url TEXT", ())
            .await;
    }

    if current_version < 10 {
        // One-time heal of FTS indexes left stale by earlier engine versions /
        // the table-rebuild migration (they returned 0.0 scores and an unreliable
        // match set). turso maintains the index incrementally on writes, so this
        // only needs to run once — hence a versioned migration, not every open.
        rebuild_fts_index(conn).await;
    }

    if current_version < 12 {
        // Canonicalize existing `doi` values to one stored form. Earlier importers
        // wrote the same arXiv paper as both `arXiv:X` and `10.48550/arXiv.X`;
        // writes now go through `Paper::canonical_doi`, but pre-existing rows still
        // hold the raw form. This backfills them so the column is uniform.
        backfill_canonical_dois(conn).await;
    }

    if current_version < 13 {
        // Citation-relationship storage + one-time-task bookkeeping. The tables
        // are created here for existing DBs (CREATE_TABLES handles fresh ones).
        // Population happens in the app layer on startup (needs pdfium), guarded
        // by an `app_flags` row.
        let _ = conn
            .execute(
                "CREATE TABLE IF NOT EXISTS paper_citations (
                    citing_paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
                    cited_paper_id  TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
                    PRIMARY KEY (citing_paper_id, cited_paper_id)
                )",
                (),
            )
            .await;
        let _ = conn
            .execute(
                "CREATE TABLE IF NOT EXISTS app_flags (key TEXT PRIMARY KEY, value TEXT)",
                (),
            )
            .await;
    }

    // The Zotero `item_type` column is added by the idempotent ensure block
    // above (it must run even for DBs whose version counter already reached 11
    // without the column landing). The matching CRR clock backfill — so existing
    // rows sync the new column — runs in `Database::open` via recrr's
    // `migrate_add_column`, which needs the `Crr` store, not just this connection.

    if current_version < 14 {
        migrate_to_lww(conn).await?;
    }

    if current_version < SCHEMA_VERSION {
        conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])
            .await?;
    }

    Ok(())
}

/// Migrate all tables from INTEGER to TEXT primary keys (UUIDs).
async fn migrate_to_text_ids(conn: &Connection) -> Result<(), turso::Error> {
    let _ = conn
        .execute(
            "CREATE TABLE IF NOT EXISTS crr_site_id (site_id BLOB PRIMARY KEY)",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE IF NOT EXISTS crr_db_version (version INTEGER NOT NULL)",
            (),
        )
        .await;
    let _ = conn
        .execute("INSERT INTO crr_db_version (version) VALUES (0)", ())
        .await;

    let _ = conn
        .execute(
            "INSERT OR IGNORE INTO crr_site_id (site_id) VALUES (randomblob(16))",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE _id_map_papers AS SELECT id AS old_id, lower(hex(randomblob(16))) AS new_id FROM papers",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE papers_new (
                id            TEXT PRIMARY KEY,
                title         TEXT NOT NULL DEFAULT '',
                authors       TEXT NOT NULL DEFAULT '[]',
                year          INTEGER,
                doi           TEXT,
                abstract_text TEXT,
                journal       TEXT,
                volume        TEXT,
                issue         TEXT,
                pages         TEXT,
                publisher     TEXT,
                url           TEXT,
                pdf_path      TEXT,
                date_added    TEXT NOT NULL,
                date_modified TEXT NOT NULL,
                is_favorite   INTEGER NOT NULL DEFAULT 0,
                is_read       INTEGER NOT NULL DEFAULT 0,
                extra_meta    TEXT,
                fulltext      TEXT,
                citation_count INTEGER,
                citation_key  TEXT
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO papers_new (id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, fulltext, citation_count, citation_key)
             SELECT m.new_id, p.title, p.authors, p.year, p.doi, p.abstract_text, p.journal, p.volume, p.issue, p.pages, p.publisher, p.url, p.pdf_path, p.date_added, p.date_modified, p.is_favorite, p.is_read, p.extra_meta, p.fulltext, p.citation_count, p.citation_key
             FROM papers p JOIN _id_map_papers m ON p.id = m.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE _id_map_collections AS SELECT id AS old_id, lower(hex(randomblob(16))) AS new_id FROM collections",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE collections_new (
                id        TEXT PRIMARY KEY,
                name      TEXT NOT NULL,
                parent_id TEXT REFERENCES collections_new(id) ON DELETE CASCADE,
                position  INTEGER NOT NULL DEFAULT 0
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO collections_new (id, name, parent_id, position)
             SELECT m.new_id, c.name, pm.new_id, c.position
             FROM collections c
             JOIN _id_map_collections m ON c.id = m.old_id
             LEFT JOIN _id_map_collections pm ON c.parent_id = pm.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE _id_map_tags AS SELECT id AS old_id, lower(hex(randomblob(16))) AS new_id FROM tags",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE tags_new (
                id    TEXT PRIMARY KEY,
                name  TEXT NOT NULL UNIQUE,
                color TEXT
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO tags_new (id, name, color)
             SELECT m.new_id, t.name, t.color
             FROM tags t JOIN _id_map_tags m ON t.id = m.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE _id_map_annotations AS SELECT id AS old_id, lower(hex(randomblob(16))) AS new_id FROM annotations",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE annotations_new (
                id          TEXT PRIMARY KEY,
                paper_id    TEXT NOT NULL REFERENCES papers_new(id) ON DELETE CASCADE,
                page        INTEGER NOT NULL,
                ann_type    TEXT NOT NULL,
                color       TEXT NOT NULL DEFAULT '#ffff00',
                content     TEXT,
                geometry    TEXT NOT NULL,
                created_at  TEXT NOT NULL,
                modified_at TEXT NOT NULL
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO annotations_new (id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at)
             SELECT m.new_id, pm.new_id, a.page, a.ann_type, a.color, a.content, a.geometry, a.created_at, a.modified_at
             FROM annotations a
             JOIN _id_map_annotations m ON a.id = m.old_id
             JOIN _id_map_papers pm ON a.paper_id = pm.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE _id_map_notes AS SELECT id AS old_id, lower(hex(randomblob(16))) AS new_id FROM notes",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE notes_new (
                id          TEXT PRIMARY KEY,
                paper_id    TEXT NOT NULL REFERENCES papers_new(id) ON DELETE CASCADE,
                title       TEXT NOT NULL DEFAULT '',
                body        TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL,
                modified_at TEXT NOT NULL
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO notes_new (id, paper_id, title, body, created_at, modified_at)
             SELECT m.new_id, pm.new_id, n.title, n.body, n.created_at, n.modified_at
             FROM notes n
             JOIN _id_map_notes m ON n.id = m.old_id
             JOIN _id_map_papers pm ON n.paper_id = pm.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE _id_map_saved_searches AS SELECT id AS old_id, lower(hex(randomblob(16))) AS new_id FROM saved_searches",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "CREATE TABLE saved_searches_new (
                id         TEXT PRIMARY KEY,
                name       TEXT NOT NULL,
                query      TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO saved_searches_new (id, name, query, created_at)
             SELECT m.new_id, s.name, s.query, s.created_at
             FROM saved_searches s JOIN _id_map_saved_searches m ON s.id = m.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE paper_collections_new (
                paper_id      TEXT NOT NULL REFERENCES papers_new(id) ON DELETE CASCADE,
                collection_id TEXT NOT NULL REFERENCES collections_new(id) ON DELETE CASCADE,
                PRIMARY KEY (paper_id, collection_id)
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO paper_collections_new (paper_id, collection_id)
             SELECT pm.new_id, cm.new_id
             FROM paper_collections pc
             JOIN _id_map_papers pm ON pc.paper_id = pm.old_id
             JOIN _id_map_collections cm ON pc.collection_id = cm.old_id",
            (),
        )
        .await;

    let _ = conn
        .execute(
            "CREATE TABLE paper_tags_new (
                paper_id TEXT NOT NULL REFERENCES papers_new(id) ON DELETE CASCADE,
                tag_id   TEXT NOT NULL REFERENCES tags_new(id) ON DELETE CASCADE,
                PRIMARY KEY (paper_id, tag_id)
            )",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT INTO paper_tags_new (paper_id, tag_id)
             SELECT pm.new_id, tm.new_id
             FROM paper_tags pt
             JOIN _id_map_papers pm ON pt.paper_id = pm.old_id
             JOIN _id_map_tags tm ON pt.tag_id = tm.old_id",
            (),
        )
        .await;

    // Drop FTS index first (references old papers table)
    let _ = conn
        .execute("DROP INDEX IF EXISTS idx_papers_fts", ())
        .await;

    // Drop in dependency order: junctions, then FK dependents, then main tables
    let _ = conn
        .execute("DROP TABLE IF EXISTS paper_collections", ())
        .await;
    let _ = conn.execute("DROP TABLE IF EXISTS paper_tags", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS annotations", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS notes", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS papers", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS collections", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS tags", ()).await;
    let _ = conn
        .execute("DROP TABLE IF EXISTS saved_searches", ())
        .await;

    let _ = conn
        .execute("ALTER TABLE papers_new RENAME TO papers", ())
        .await;
    let _ = conn
        .execute("ALTER TABLE collections_new RENAME TO collections", ())
        .await;
    let _ = conn
        .execute("ALTER TABLE tags_new RENAME TO tags", ())
        .await;
    let _ = conn
        .execute("ALTER TABLE annotations_new RENAME TO annotations", ())
        .await;
    let _ = conn
        .execute("ALTER TABLE notes_new RENAME TO notes", ())
        .await;
    let _ = conn
        .execute(
            "ALTER TABLE saved_searches_new RENAME TO saved_searches",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "ALTER TABLE paper_collections_new RENAME TO paper_collections",
            (),
        )
        .await;
    let _ = conn
        .execute("ALTER TABLE paper_tags_new RENAME TO paper_tags", ())
        .await;

    let _ = conn
        .execute("DROP TABLE IF EXISTS _id_map_papers", ())
        .await;
    let _ = conn
        .execute("DROP TABLE IF EXISTS _id_map_collections", ())
        .await;
    let _ = conn.execute("DROP TABLE IF EXISTS _id_map_tags", ()).await;
    let _ = conn
        .execute("DROP TABLE IF EXISTS _id_map_annotations", ())
        .await;
    let _ = conn.execute("DROP TABLE IF EXISTS _id_map_notes", ()).await;
    let _ = conn
        .execute("DROP TABLE IF EXISTS _id_map_saved_searches", ())
        .await;

    let _ = conn.execute(CREATE_FTS_INDEX, ()).await;

    Ok(())
}

/// The schema version recorded in the database.
///
/// `Ok(0)` means genuinely fresh — the `schema_version` table has no row yet.
/// A read failure is an error rather than 0, because those routed a populated
/// database into the fresh-database branch, which inserts a second
/// `schema_version` row and leaves the real version permanently ambiguous under
/// `LIMIT 1`.
pub async fn get_schema_version(conn: &Connection) -> Result<i64, turso::Error> {
    // The table is created before this runs, so a query error here is a real
    // driver failure and not "the schema does not exist yet".
    let mut rows = conn
        .query("SELECT version FROM schema_version LIMIT 1", ())
        .await?;

    match rows.next().await? {
        Some(row) => Ok(row
            .get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0)),
        None => Ok(0),
    }
}

/// Add the last-writer-wins bookkeeping columns to every synced table.
///
/// `updated_at` (unix millis) and `updated_by` (device id) are compared as a
/// tuple to resolve a merge; `deleted` is a tombstone flag so a delete has
/// something to publish. No table carried a usable equivalent: `papers` had
/// `date_modified` and `annotations`/`notes` had `modified_at`, but those are
/// user-visible edit times that sync must not perturb, and `collections`,
/// `tags`, and the three junction tables had no timestamp at all.
async fn migrate_to_lww(conn: &Connection) -> Result<(), turso::Error> {
    // Seed rows that have no timestamp to source from a day in the past.
    //
    // Seeding at `now` would mean the second device to migrate outranks the
    // first on every row, so its copy of every collection name and tag would
    // silently win the whole library. Backdating a fixed amount keeps migration
    // seeds below any genuine post-migration edit; ties among themselves fall to
    // `updated_by`, which is deterministic. This mirrors what the recrr backfill
    // does on purpose — adopted rows seed at `col_ver = 1` so a real edit wins.
    const BACKDATE_MS: i64 = 86_400_000;

    let now_ms = chrono::Utc::now().timestamp_millis();
    let seed_ms = now_ms - BACKDATE_MS;

    let device_id = device_id_hex(conn).await?;

    for table in crate::sync_schema::SYNCED_TABLES {
        // Idempotent: re-running a partly-applied migration must not fail.
        for (column, decl) in [
            ("updated_at", "INTEGER NOT NULL DEFAULT 0"),
            ("updated_by", "TEXT NOT NULL DEFAULT ''"),
            ("deleted", "INTEGER NOT NULL DEFAULT 0"),
        ] {
            let _ = conn
                .execute(
                    &format!("ALTER TABLE {} ADD COLUMN {column} {decl}", table.name),
                    (),
                )
                .await;
        }

        // Unlike the ALTERs, these must not be swallowed. A row left at
        // `updated_at = 0` loses every comparison forever, so a half-applied
        // backfill is silent, permanent data loss rather than a retryable error.
        conn.execute(
            &format!(
                "UPDATE {} SET updated_by = ?1 WHERE updated_by = ''",
                table.name
            ),
            [turso::Value::Text(device_id.clone())],
        )
        .await?;

        conn.execute(
            &format!(
                "UPDATE {} SET updated_at = ?1 WHERE updated_at = 0",
                table.name
            ),
            [turso::Value::Integer(seed_ms)],
        )
        .await?;

        let _ = conn
            .execute(
                &format!(
                    "CREATE INDEX IF NOT EXISTS idx_{}_updated ON {} (updated_at)",
                    table.name, table.name
                ),
                (),
            )
            .await;
    }

    // Prefer each row's own edit time where the table records one, so a paper
    // edited long ago does not outrank one edited yesterday.
    for (table, column) in [
        ("papers", "date_modified"),
        ("annotations", "modified_at"),
        ("notes", "modified_at"),
        ("saved_searches", "created_at"),
    ] {
        seed_from_timestamp(conn, table, column, seed_ms).await?;
    }

    create_live_views(conn).await;

    Ok(())
}

/// This device's id as lowercase hex, creating one if the table is empty.
async fn device_id_hex(conn: &Connection) -> Result<String, turso::Error> {
    let _ = conn
        .execute(
            "CREATE TABLE IF NOT EXISTS crr_site_id (site_id BLOB PRIMARY KEY)",
            (),
        )
        .await;
    let _ = conn
        .execute(
            "INSERT OR IGNORE INTO crr_site_id (site_id) VALUES (randomblob(16))",
            (),
        )
        .await;

    let mut rows = conn
        .query("SELECT lower(hex(site_id)) FROM crr_site_id LIMIT 1", ())
        .await?;
    Ok(rows
        .next()
        .await?
        .and_then(|r| r.get_value(0).ok())
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default())
}

/// Replace seeded `updated_at` values with each row's own RFC3339 timestamp.
///
/// Parsed in Rust rather than SQL: these are RFC3339 strings with offsets, and
/// relying on turso's `strftime`/`unixepoch` handling of those is not a bet
/// worth making inside a migration that cannot be re-run cleanly. Rows whose
/// timestamp does not parse keep the backdated seed, which is the safe side —
/// they lose to real edits rather than winning over them.
async fn seed_from_timestamp(
    conn: &Connection,
    table: &str,
    column: &str,
    seed_ms: i64,
) -> Result<(), turso::Error> {
    let mut rows = conn
        .query(
            &format!("SELECT id, {column} FROM {table} WHERE updated_at = ?1"),
            [turso::Value::Integer(seed_ms)],
        )
        .await?;

    let mut updates = Vec::new();
    while let Some(row) = rows.next().await? {
        let (Some(id), Some(ts)) = (
            row.get_value(0).ok().and_then(|v| v.as_text().cloned()),
            row.get_value(1).ok().and_then(|v| v.as_text().cloned()),
        ) else {
            continue;
        };
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&ts) {
            updates.push((id, parsed.timestamp_millis()));
        }
    }

    for (id, ms) in updates {
        conn.execute(
            &format!("UPDATE {table} SET updated_at = ?1 WHERE id = ?2"),
            turso::params::Params::Positional(vec![
                turso::Value::Integer(ms),
                turso::Value::Text(id),
            ]),
        )
        .await?;
    }

    Ok(())
}

/// Create the `<table>_live` views, ignoring ones that already exist.
///
/// turso rejects `CREATE VIEW ... IF NOT EXISTS` with "View ... already exists"
/// rather than treating it as a no-op, so each statement is attempted on its own
/// and an existing view is not an error.
async fn create_live_views(conn: &Connection) {
    for stmt in CREATE_LIVE_VIEWS {
        let _ = conn.execute(*stmt, ()).await;
    }
}

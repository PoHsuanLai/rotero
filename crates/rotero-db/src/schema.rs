use turso::Connection;

const SCHEMA_VERSION: i64 = 9;

const CREATE_FTS_INDEX: &str = "CREATE INDEX IF NOT EXISTS idx_papers_fts ON papers \
     USING fts (title, authors, abstract_text, journal, fulltext) \
     WITH (weights = 'title=3.0,authors=2.0,abstract_text=1.5,journal=1.0,fulltext=1.0')";

const CREATE_TABLES: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS papers (
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
    citation_key  TEXT,
    pdf_url       TEXT
);

CREATE TABLE IF NOT EXISTS collections (
    id        TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    parent_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS paper_collections (
    paper_id      TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (paper_id, collection_id)
);

CREATE TABLE IF NOT EXISTS tags (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    color TEXT
);

CREATE TABLE IF NOT EXISTS paper_tags (
    paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (paper_id, tag_id)
);

CREATE TABLE IF NOT EXISTS annotations (
    id          TEXT PRIMARY KEY,
    paper_id    TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    page        INTEGER NOT NULL,
    ann_type    TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '#ffff00',
    content     TEXT,
    geometry    TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    modified_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id          TEXT PRIMARY KEY,
    paper_id    TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    modified_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS saved_searches (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    query      TEXT NOT NULL,
    created_at TEXT NOT NULL
);
";

pub async fn initialize_db(conn: &Connection) -> Result<(), turso::Error> {
    conn.execute_batch(CREATE_TABLES).await?;

    // Run migrations for existing databases
    run_migrations(conn).await?;

    // Ensure CRR clock tables exist (idempotent)
    crate::crr::init_crr_tables(conn).await?;

    Ok(())
}

async fn run_migrations(conn: &Connection) -> Result<(), turso::Error> {
    let current_version = get_schema_version(conn).await;

    if current_version < 1 {
        // Fresh database — create FTS index and set the version
        let _ = conn.execute(CREATE_FTS_INDEX, ()).await;
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )
        .await?;
        return Ok(());
    }

    // Migration from v1 to v2: add is_favorite and is_read columns
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

    // Migration to v3: add fulltext column for PDF content search
    if current_version < 3 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN fulltext TEXT", ())
            .await;
    }

    // Migration to v4: add citation_count column + saved_searches table
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

    // Migration to v5: add FTS index for full-text search
    if current_version < 5 {
        let _ = conn.execute(CREATE_FTS_INDEX, ()).await;
    }

    // Migration to v6: add citation_key column for Better BibTeX
    if current_version < 6 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN citation_key TEXT", ())
            .await;
    }

    // Migration to v7: backfill NULL tag colors with palette colors
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

    // Ensure citation_count column exists (may have been missed if v4 migration partially ran)
    let _ = conn
        .execute("ALTER TABLE papers ADD COLUMN citation_count INTEGER", ())
        .await;

    // Ensure pdf_url column exists
    let _ = conn
        .execute("ALTER TABLE papers ADD COLUMN pdf_url TEXT", ())
        .await;

    // Migration to v8: convert INTEGER PRIMARY KEY to TEXT PRIMARY KEY (String UUIDs)
    if current_version < 8 {
        migrate_to_text_ids(conn).await?;
    }

    // Migration to v9: add pdf_url column
    if current_version < 9 {
        let _ = conn
            .execute("ALTER TABLE papers ADD COLUMN pdf_url TEXT", ())
            .await;
    }

    if current_version < SCHEMA_VERSION {
        conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])
            .await?;
    }

    Ok(())
}

/// Migrate all tables from INTEGER PRIMARY KEY to TEXT PRIMARY KEY.
/// Generates random UUIDs for existing rows using hex(randomblob(16)).
async fn migrate_to_text_ids(conn: &Connection) -> Result<(), turso::Error> {
    // Create CRR metadata tables
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

    // Generate and insert this device's site UUID
    let _ = conn
        .execute(
            "INSERT OR IGNORE INTO crr_site_id (site_id) VALUES (randomblob(16))",
            (),
        )
        .await;

    // --- papers ---
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
             SELECT lower(hex(randomblob(16))), title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, fulltext, citation_count, citation_key
             FROM papers",
            (),
        )
        .await;

    // Build mapping table for papers: old integer id -> new text id
    let _ = conn
        .execute(
            "CREATE TABLE _id_map_papers (old_id INTEGER PRIMARY KEY, new_id TEXT NOT NULL)",
            (),
        )
        .await;
    // We need to correlate old rows with new rows. Use rowid ordering via date_added + title as a key.
    // Simpler approach: re-read old papers and assign from papers_new by matching on unique columns.
    // Actually, the cleanest approach: create the mapping BEFORE inserting into papers_new.
    // Let's drop papers_new, create the mapping first, then re-insert.
    let _ = conn.execute("DROP TABLE IF EXISTS papers_new", ()).await;
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

    // --- collections ---
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

    // --- tags ---
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

    // --- annotations ---
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

    // --- notes ---
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

    // --- saved_searches ---
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

    // --- paper_collections (junction table) ---
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

    // --- paper_tags (junction table) ---
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

    // --- Drop old tables and rename new ones ---
    // Drop FTS index first (it references old papers table)
    let _ = conn
        .execute("DROP INDEX IF EXISTS idx_papers_fts", ())
        .await;

    // Drop junction tables first (they reference the main tables)
    let _ = conn
        .execute("DROP TABLE IF EXISTS paper_collections", ())
        .await;
    let _ = conn.execute("DROP TABLE IF EXISTS paper_tags", ()).await;
    // Drop tables that reference papers
    let _ = conn.execute("DROP TABLE IF EXISTS annotations", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS notes", ()).await;
    // Drop main tables
    let _ = conn.execute("DROP TABLE IF EXISTS papers", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS collections", ()).await;
    let _ = conn.execute("DROP TABLE IF EXISTS tags", ()).await;
    let _ = conn
        .execute("DROP TABLE IF EXISTS saved_searches", ())
        .await;

    // Rename new tables
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

    // Drop mapping tables
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

    // Rebuild FTS index on the new papers table
    let _ = conn.execute(CREATE_FTS_INDEX, ()).await;

    Ok(())
}

async fn get_schema_version(conn: &Connection) -> i64 {
    let result = conn
        .query("SELECT version FROM schema_version LIMIT 1", ())
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

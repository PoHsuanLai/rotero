use turso::Connection;

const SCHEMA_VERSION: i64 = 7;

const CREATE_FTS_INDEX: &str = "CREATE INDEX IF NOT EXISTS idx_papers_fts ON papers \
     USING fts (title, authors, abstract_text, journal, fulltext) \
     WITH (weights = 'title=3.0,authors=2.0,abstract_text=1.5,journal=1.0,fulltext=1.0')";

const CREATE_TABLES: &str = "
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS papers (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
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
    fulltext      TEXT
);

CREATE TABLE IF NOT EXISTS collections (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    name      TEXT NOT NULL,
    parent_id INTEGER REFERENCES collections(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS paper_collections (
    paper_id      INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    PRIMARY KEY (paper_id, collection_id)
);

CREATE TABLE IF NOT EXISTS tags (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE,
    color TEXT
);

CREATE TABLE IF NOT EXISTS paper_tags (
    paper_id INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (paper_id, tag_id)
);

CREATE TABLE IF NOT EXISTS annotations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    paper_id    INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    page        INTEGER NOT NULL,
    ann_type    TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '#ffff00',
    content     TEXT,
    geometry    TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    modified_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS notes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    paper_id    INTEGER NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    modified_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS saved_searches (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    name       TEXT NOT NULL,
    query      TEXT NOT NULL,
    created_at TEXT NOT NULL
);
";

pub async fn initialize_db(conn: &Connection) -> Result<(), turso::Error> {
    conn.execute_batch(CREATE_TABLES).await?;

    // Run migrations for existing databases
    run_migrations(conn).await?;

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
            "#6b7085", "#7c6b85", "#6b8580", "#857a6b",
            "#6b7a85", "#856b7a", "#6b856e", "#85706b",
            "#6e6b85", "#7a856b", "#856b6b", "#6b8585",
        ];
        let mut rows = conn
            .query(super::queries::TAG_LIST_NULL_COLOR, ())
            .await?;
        let mut updates: Vec<(i64, String)> = Vec::new();
        while let Some(row) = rows.next().await? {
            let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
            let name = row.get_value(1)?.as_text().cloned().unwrap_or_default();
            let hash = name.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
            updates.push((id, PALETTE[hash % PALETTE.len()].to_string()));
        }
        for (id, color) in updates {
            let _ = conn
                .execute(
                    super::queries::TAG_UPDATE_COLOR,
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

    if current_version < SCHEMA_VERSION {
        conn.execute("UPDATE schema_version SET version = ?1", [SCHEMA_VERSION])
            .await?;
    }

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

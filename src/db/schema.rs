use turso::Connection;

const SCHEMA_VERSION: i64 = 2;

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
    extra_meta    TEXT
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
";

const CREATE_FTS_INDEX: &str = "
CREATE INDEX IF NOT EXISTS idx_papers_fts ON papers USING fts (title, authors, abstract_text, journal);
";

pub async fn initialize_db(conn: &Connection) -> Result<(), turso::Error> {
    conn.execute_batch(CREATE_TABLES).await?;

    // Run migrations for existing databases
    run_migrations(conn).await?;

    // Create FTS index (turso-specific)
    if let Err(e) = conn.execute_batch(CREATE_FTS_INDEX).await {
        eprintln!("FTS index creation skipped: {e}");
    }

    Ok(())
}

async fn run_migrations(conn: &Connection) -> Result<(), turso::Error> {
    let current_version = get_schema_version(conn).await;

    if current_version < 1 {
        // Fresh database — just set the version
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )
        .await?;
        return Ok(());
    }

    // Migration from v1 to v2: add is_favorite and is_read columns
    if current_version < 2 {
        // ALTER TABLE may fail if columns already exist (e.g. fresh DB) — that's fine
        let _ = conn.execute("ALTER TABLE papers ADD COLUMN is_favorite INTEGER NOT NULL DEFAULT 0", ()).await;
        let _ = conn.execute("ALTER TABLE papers ADD COLUMN is_read INTEGER NOT NULL DEFAULT 0", ()).await;

        conn.execute(
            "UPDATE schema_version SET version = ?1",
            [SCHEMA_VERSION],
        )
        .await?;
    }

    Ok(())
}

async fn get_schema_version(conn: &Connection) -> i64 {
    let result = conn.query("SELECT version FROM schema_version LIMIT 1", ()).await;
    match result {
        Ok(mut rows) => {
            if let Ok(Some(row)) = rows.next().await {
                row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0)
            } else {
                0
            }
        }
        Err(_) => 0,
    }
}

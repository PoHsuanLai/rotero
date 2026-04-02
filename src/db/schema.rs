use rusqlite::Connection;

const SCHEMA_VERSION: u32 = 1;

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

CREATE INDEX IF NOT EXISTS idx_papers_doi ON papers(doi);
CREATE INDEX IF NOT EXISTS idx_papers_date_added ON papers(date_added);
CREATE INDEX IF NOT EXISTS idx_collections_parent ON collections(parent_id);
CREATE INDEX IF NOT EXISTS idx_annotations_paper ON annotations(paper_id);
CREATE INDEX IF NOT EXISTS idx_notes_paper ON notes(paper_id);
";

pub fn initialize_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    conn.execute_batch(CREATE_TABLES)?;

    // Check/set schema version
    let version: Option<u32> = conn
        .query_row(
            "SELECT version FROM schema_version LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if version.is_none() {
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            [SCHEMA_VERSION],
        )?;
    }

    Ok(())
}

//! CREATE TABLE statements for all core tables.

/// SQL batch that creates all core tables if they do not already exist.
pub const CREATE_TABLES: &str = "
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
    pdf_url       TEXT,
    item_type     TEXT NOT NULL DEFAULT 'journalArticle',
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS collections (
    id        TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    parent_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS paper_collections (
    paper_id      TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (paper_id, collection_id)
);

CREATE TABLE IF NOT EXISTS tags (
    id    TEXT PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    color TEXT,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS paper_tags (
    paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (paper_id, tag_id)
);

CREATE TABLE IF NOT EXISTS paper_citations (
    citing_paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    cited_paper_id  TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (citing_paper_id, cited_paper_id)
);

-- Bookkeeping for one-time app tasks (e.g. the initial citation scan), keyed by
-- a task name. Local-only: not replicated, since it records what THIS install has
-- done, not shared library state.
CREATE TABLE IF NOT EXISTS app_flags (
    key   TEXT PRIMARY KEY,
    value TEXT
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
    modified_at TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS notes (
    id          TEXT PRIMARY KEY,
    paper_id    TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
    title       TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS saved_searches (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    query      TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT NOT NULL DEFAULT '',
    deleted    INTEGER NOT NULL DEFAULT 0
);
";

/// SQL statement that creates the turso FTS index over paper text fields with weighted columns.
/// Views exposing only live (non-tombstoned) rows of each synced table.
///
/// Reads go through `<table>_live` so a query that forgets `deleted = 0` names a
/// relation that does not exist — a compile-time-ish failure — rather than
/// silently returning deleted papers. Writes still target the base tables.
///
/// `papers_live` deliberately has no FTS index: `idx_papers_fts` is built on
/// `papers` and cannot be filtered, so full-text search matches tombstoned rows
/// and must drop them after the match.
/// Views exposing only live (non-tombstoned) rows of each synced table.
///
/// Reads go through `<table>_live` so a query that forgets `deleted = 0` names a
/// relation that does not exist, rather than silently returning deleted papers.
/// Writes still target the base tables.
///
/// `papers_live` deliberately has no FTS index: `idx_papers_fts` is built on
/// `papers` and cannot be filtered, so full-text search matches tombstoned rows
/// and must drop them after the match.
///
/// One statement per view rather than a batch: turso raises "View ... already
/// exists" even with `IF NOT EXISTS`, so each is attempted separately and an
/// already-created view is ignored.
pub const CREATE_LIVE_VIEWS: &[&str] = &[
    "CREATE VIEW papers_live AS SELECT * FROM papers WHERE deleted = 0",
    "CREATE VIEW collections_live AS SELECT * FROM collections WHERE deleted = 0",
    "CREATE VIEW tags_live AS SELECT * FROM tags WHERE deleted = 0",
    "CREATE VIEW annotations_live AS SELECT * FROM annotations WHERE deleted = 0",
    "CREATE VIEW notes_live AS SELECT * FROM notes WHERE deleted = 0",
    "CREATE VIEW saved_searches_live AS SELECT * FROM saved_searches WHERE deleted = 0",
    "CREATE VIEW paper_collections_live AS SELECT * FROM paper_collections WHERE deleted = 0",
    "CREATE VIEW paper_tags_live AS SELECT * FROM paper_tags WHERE deleted = 0",
    "CREATE VIEW paper_citations_live AS SELECT * FROM paper_citations WHERE deleted = 0",
];

pub const CREATE_FTS_INDEX: &str = "CREATE INDEX IF NOT EXISTS idx_papers_fts ON papers \
     USING fts (title, authors, abstract_text, journal, fulltext) \
     WITH (weights = 'title=3.0,authors=2.0,abstract_text=1.5,journal=1.0,fulltext=1.0')";

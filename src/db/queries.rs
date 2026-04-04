// ---------------------------------------------------------------------------
// Papers
// ---------------------------------------------------------------------------

pub const PAPER_SELECT_COLS: &str = "id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key";

pub const PAPER_INSERT: &str = "\
    INSERT INTO papers (title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)";

pub const PAPER_COUNT: &str = "SELECT COUNT(*) FROM papers";

pub const PAPER_SEARCH_FTS: &str = "\
    SELECT {COLS}, fts_score(title, authors, abstract_text, journal, fulltext, ?1) AS score \
    FROM papers \
    WHERE (title, authors, abstract_text, journal, fulltext) MATCH ?1 OR doi = ?1 \
    ORDER BY score DESC \
    LIMIT 50";

pub const PAPER_SEARCH_LIKE: &str = "\
    SELECT {COLS} FROM papers \
    WHERE title LIKE ?1 OR authors LIKE ?1 OR abstract_text LIKE ?1 OR journal LIKE ?1 OR doi LIKE ?1 OR fulltext LIKE ?1 \
    ORDER BY date_added DESC LIMIT 50";

pub const PAPER_SET_FAVORITE: &str = "UPDATE papers SET is_favorite = ?1 WHERE id = ?2";
pub const PAPER_SET_READ: &str = "UPDATE papers SET is_read = ?1 WHERE id = ?2";
pub const PAPER_UPDATE_FULLTEXT: &str = "UPDATE papers SET fulltext = ?1 WHERE id = ?2";

pub const PAPER_UPDATE_METADATA: &str = "\
    UPDATE papers SET title = ?1, authors = ?2, year = ?3, doi = ?4, abstract_text = ?5, \
    journal = ?6, volume = ?7, issue = ?8, pages = ?9, publisher = ?10, url = ?11, \
    date_modified = ?12 WHERE id = ?13";

pub const PAPER_UPDATE_PDF_PATH: &str = "UPDATE papers SET pdf_path = ?1, date_modified = ?2 WHERE id = ?3";
pub const PAPER_DELETE: &str = "DELETE FROM papers WHERE id = ?1";

pub const PAPER_FIND_DOI_DUPLICATES: &str = "\
    SELECT {COLS} FROM papers WHERE doi IS NOT NULL AND doi != '' \
    AND doi IN (SELECT doi FROM papers WHERE doi IS NOT NULL AND doi != '' GROUP BY doi HAVING COUNT(*) > 1) \
    ORDER BY doi, date_added DESC";

pub const PAPER_MERGE_COLLECTIONS: &str = "\
    INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) \
    SELECT ?1, collection_id FROM paper_collections WHERE paper_id = ?2";

pub const PAPER_MERGE_TAGS: &str = "\
    INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) \
    SELECT ?1, tag_id FROM paper_tags WHERE paper_id = ?2";

pub const PAPER_LIST_NEEDING_CITATIONS: &str = "\
    SELECT id, doi FROM papers WHERE doi IS NOT NULL AND citation_count IS NULL";

pub const PAPER_UPDATE_CITATION_COUNT: &str = "UPDATE papers SET citation_count = ?1 WHERE id = ?2";
pub const PAPER_UPDATE_CITATION_KEY: &str = "UPDATE papers SET citation_key = ?1 WHERE id = ?2";

pub const PAPER_LIST_NEEDING_CITATION_KEYS: &str = "\
    SELECT id, title, authors, year FROM papers \
    WHERE citation_key IS NULL AND title != '' AND title != 'Untitled'";

pub const PAPER_LIST_CITATION_KEYS: &str = "SELECT citation_key FROM papers WHERE citation_key IS NOT NULL";

// ---------------------------------------------------------------------------
// Collections
// ---------------------------------------------------------------------------

pub const COLLECTION_INSERT: &str = "INSERT INTO collections (name, parent_id, position) VALUES (?1, ?2, ?3)";

pub const COLLECTION_LIST: &str = "\
    SELECT id, name, parent_id, position FROM collections ORDER BY parent_id NULLS FIRST, position";

pub const COLLECTION_RENAME: &str = "UPDATE collections SET name = ?1 WHERE id = ?2";
pub const COLLECTION_REPARENT: &str = "UPDATE collections SET parent_id = ?1 WHERE id = ?2";
pub const COLLECTION_DELETE: &str = "DELETE FROM collections WHERE id = ?1";

pub const COLLECTION_PAPER_IDS: &str = "SELECT paper_id FROM paper_collections WHERE collection_id = ?1";
pub const COLLECTION_ADD_PAPER: &str = "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) VALUES (?1, ?2)";
pub const COLLECTION_REMOVE_PAPER: &str = "DELETE FROM paper_collections WHERE paper_id = ?1 AND collection_id = ?2";

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

pub const TAG_FIND_BY_NAME: &str = "SELECT id FROM tags WHERE name = ?1";
pub const TAG_INSERT: &str = "INSERT INTO tags (name, color) VALUES (?1, ?2)";
pub const TAG_LIST: &str = "SELECT id, name, color FROM tags ORDER BY name";
pub const TAG_ADD_TO_PAPER: &str = "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)";
pub const TAG_RENAME: &str = "UPDATE tags SET name = ?1 WHERE id = ?2";
pub const TAG_UPDATE_COLOR: &str = "UPDATE tags SET color = ?1 WHERE id = ?2";
pub const TAG_LIST_NULL_COLOR: &str = "SELECT id, name FROM tags WHERE color IS NULL";
pub const TAG_PAPER_IDS: &str = "SELECT paper_id FROM paper_tags WHERE tag_id = ?1";
pub const TAG_DELETE: &str = "DELETE FROM tags WHERE id = ?1";

// ---------------------------------------------------------------------------
// Annotations
// ---------------------------------------------------------------------------

pub const ANNOTATION_INSERT: &str = "\
    INSERT INTO annotations (paper_id, page, ann_type, color, content, geometry, created_at, modified_at) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)";

pub const ANNOTATION_LIST_FOR_PAPER: &str = "\
    SELECT id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at \
    FROM annotations WHERE paper_id = ?1 ORDER BY page, created_at";

pub const ANNOTATION_UPDATE_CONTENT: &str = "UPDATE annotations SET content = ?1, modified_at = ?2 WHERE id = ?3";
pub const ANNOTATION_UPDATE_COLOR: &str = "UPDATE annotations SET color = ?1, modified_at = ?2 WHERE id = ?3";
pub const ANNOTATION_DELETE: &str = "DELETE FROM annotations WHERE id = ?1";

// ---------------------------------------------------------------------------
// Notes
// ---------------------------------------------------------------------------

pub const NOTE_INSERT: &str = "\
    INSERT INTO notes (paper_id, title, body, created_at, modified_at) \
    VALUES (?1, ?2, ?3, ?4, ?5)";

pub const NOTE_LIST_FOR_PAPER: &str = "\
    SELECT id, paper_id, title, body, created_at, modified_at \
    FROM notes WHERE paper_id = ?1 ORDER BY created_at DESC";

pub const NOTE_UPDATE: &str = "UPDATE notes SET title = ?1, body = ?2, modified_at = ?3 WHERE id = ?4";
pub const NOTE_DELETE: &str = "DELETE FROM notes WHERE id = ?1";

// ---------------------------------------------------------------------------
// Saved Searches
// ---------------------------------------------------------------------------

pub const SAVED_SEARCH_INSERT: &str = "INSERT INTO saved_searches (name, query, created_at) VALUES (?1, ?2, ?3)";

pub const SAVED_SEARCH_LIST: &str = "\
    SELECT id, name, query, created_at FROM saved_searches ORDER BY created_at DESC";

pub const SAVED_SEARCH_DELETE: &str = "DELETE FROM saved_searches WHERE id = ?1";
pub const SAVED_SEARCH_RENAME: &str = "UPDATE saved_searches SET name = ?1 WHERE id = ?2";

// ---------------------------------------------------------------------------
// Graph
// ---------------------------------------------------------------------------

pub const GRAPH_ALL_PAPER_TAGS: &str = "SELECT paper_id, tag_id FROM paper_tags";
pub const GRAPH_ALL_PAPER_COLLECTIONS: &str = "SELECT paper_id, collection_id FROM paper_collections";

// ---------------------------------------------------------------------------
// Common
// ---------------------------------------------------------------------------

pub const LAST_INSERT_ROWID: &str = "SELECT last_insert_rowid()";

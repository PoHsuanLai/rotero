/// SELECT columns for paper queries. `item_type` is appended last so the
/// positional `Paper::from_row` indices for the earlier columns stay fixed.
pub const PAPER_SELECT_COLS: &str = "id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key, pdf_url, item_type";

/// Insert a new paper row with all columns.
pub const PAPER_INSERT: &str = "\
    INSERT INTO papers (id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key, pdf_url, item_type) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)";

/// Count total papers in the library.
pub const PAPER_COUNT: &str = "SELECT COUNT(*) FROM papers_live";

/// Direct identifier lookup, used as a fast path when the query parses as a DOI
/// or other [`PaperId`](crate::PaperId).
// DOIs are case-insensitive by specification, so match without regard to case.
pub const PAPER_SEARCH_BY_DOI: &str =
    "SELECT {COLS} FROM papers_live WHERE doi = ?1 COLLATE NOCASE LIMIT 50";

pub const PAPER_SEARCH_BY_URL: &str =
    "SELECT {COLS} FROM papers_live WHERE url = ?1 OR pdf_url = ?1 LIMIT 50";

/// Full-text search ranked by BM25 relevance.
///
/// turso's FTS is invoked via the `fts_match()` / `fts_score()` functions (the
/// `(cols) MATCH ?` operator form is not supported and errors). Both take the
/// same argument order: the indexed columns, then the query string. The score
/// is the trailing column, read separately from the model columns.
///
/// The bound query string is expected to already be an AND-joined match
/// expression (see [`build_fts_match_query`](crate::build_fts_match_query)):
/// turso combines bare tokens with OR and keeps stopwords, so a raw multi-word
/// query would match nearly the whole library on a common word like "a".
pub const PAPER_SEARCH_FTS: &str = "\
    SELECT {COLS}, fts_score(title, authors, abstract_text, journal, fulltext, ?1) AS score \
    FROM papers_live \
    WHERE fts_match(title, authors, abstract_text, journal, fulltext, ?1) \
    ORDER BY score DESC \
    LIMIT 50";

/// Fallback LIKE-based search when FTS is unavailable (e.g. index missing).
pub const PAPER_SEARCH_LIKE: &str = "\
    SELECT {COLS} FROM papers_live \
    WHERE title LIKE ?1 OR authors LIKE ?1 OR abstract_text LIKE ?1 OR journal LIKE ?1 OR doi LIKE ?1 OR fulltext LIKE ?1 \
    LIMIT 500";

/// Toggle a paper's favorite flag.
pub const PAPER_SET_FAVORITE: &str = "UPDATE papers SET is_favorite = ?1 WHERE id = ?2";
/// Toggle a paper's read flag.
pub const PAPER_SET_READ: &str = "UPDATE papers SET is_read = ?1 WHERE id = ?2";
/// Store extracted full text for a paper.
pub const PAPER_UPDATE_FULLTEXT: &str = "UPDATE papers SET fulltext = ?1 WHERE id = ?2";

/// Update core metadata fields for a paper.
pub const PAPER_UPDATE_METADATA: &str = "\
    UPDATE papers SET title = ?1, authors = ?2, year = ?3, doi = ?4, abstract_text = ?5, \
    journal = ?6, volume = ?7, issue = ?8, pages = ?9, publisher = ?10, url = ?11, \
    date_modified = ?12, item_type = ?13 WHERE id = ?14";

/// Read a paper's extracted full text.
pub const PAPER_SELECT_FULLTEXT: &str = "SELECT fulltext FROM papers_live WHERE id = ?1";
/// Set or replace the local PDF file path for a paper.
pub const PAPER_UPDATE_PDF_PATH: &str =
    "UPDATE papers SET pdf_path = ?1, date_modified = ?2 WHERE id = ?3";
/// Set the title for a paper, leaving every other bibliographic field alone.
pub const PAPER_UPDATE_TITLE: &str =
    "UPDATE papers SET title = ?1, date_modified = ?2 WHERE id = ?3";
/// Bump the date_modified timestamp on a paper.
pub const PAPER_TOUCH: &str = "UPDATE papers SET date_modified = ?1 WHERE id = ?2";
/// Delete a paper by ID.
pub const PAPER_DELETE: &str = "DELETE FROM papers WHERE id = ?1";

/// Find all papers that share a DOI with at least one other paper.
pub const PAPER_FIND_DOI_DUPLICATES: &str = "\
    SELECT {COLS} FROM papers_live WHERE doi IS NOT NULL AND doi != '' \
    AND doi IN (SELECT doi FROM papers_live WHERE doi IS NOT NULL AND doi != '' GROUP BY doi HAVING COUNT(*) > 1) \
    ORDER BY doi, date_added DESC";

/// Copy collection memberships from one paper to another (for merging duplicates).
pub const PAPER_MERGE_COLLECTIONS: &str = "\
    INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) \
    SELECT ?1, collection_id FROM paper_collections WHERE paper_id = ?2";

/// Copy tag associations from one paper to another (for merging duplicates).
pub const PAPER_MERGE_TAGS: &str = "\
    INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) \
    SELECT ?1, tag_id FROM paper_tags WHERE paper_id = ?2";

/// List papers that have a DOI but no citation count yet.
pub const PAPER_LIST_NEEDING_CITATIONS: &str = "\
    SELECT id, doi FROM papers_live WHERE doi IS NOT NULL AND citation_count IS NULL";

/// Set the citation count for a paper.
pub const PAPER_UPDATE_CITATION_COUNT: &str = "UPDATE papers SET citation_count = ?1 WHERE id = ?2";
/// Set the citation key for a paper.
pub const PAPER_UPDATE_CITATION_KEY: &str = "UPDATE papers SET citation_key = ?1 WHERE id = ?2";

/// List papers that still need an auto-generated citation key.
pub const PAPER_LIST_NEEDING_CITATION_KEYS: &str = "\
    SELECT id, title, authors, year FROM papers_live \
    WHERE citation_key IS NULL AND title != '' AND title != 'Untitled'";

/// List all existing citation keys (for uniqueness checks).
pub const PAPER_LIST_CITATION_KEYS: &str =
    "SELECT citation_key FROM papers_live WHERE citation_key IS NOT NULL";

/// List papers that have a remote PDF URL.
pub const PAPER_SELECT_PDF_URL: &str = "SELECT id, pdf_url FROM papers_live WHERE pdf_url IS NOT NULL";

/// Insert a new collection.
pub const COLLECTION_INSERT: &str =
    "INSERT INTO collections (id, name, parent_id, position) VALUES (?1, ?2, ?3, ?4)";

/// List all collections ordered by hierarchy and position.
pub const COLLECTION_LIST: &str = "\
    SELECT id, name, parent_id, position FROM collections_live ORDER BY parent_id NULLS FIRST, position";

/// Rename a collection.
pub const COLLECTION_RENAME: &str = "UPDATE collections SET name = ?1 WHERE id = ?2";
/// Move a collection under a new parent.
pub const COLLECTION_REPARENT: &str = "UPDATE collections SET parent_id = ?1 WHERE id = ?2";
/// Delete a collection by ID.
pub const COLLECTION_DELETE: &str = "DELETE FROM collections WHERE id = ?1";

/// List paper IDs belonging directly to a single collection.
pub const COLLECTION_PAPER_IDS: &str =
    "SELECT paper_id FROM paper_collections_live WHERE collection_id = ?1";
/// List the collections a single paper belongs to, ordered by name.
pub const COLLECTION_LIST_FOR_PAPER: &str = "\
    SELECT c.id, c.name, c.parent_id, c.position FROM collections_live c \
    JOIN paper_collections_live pc ON pc.collection_id = c.id \
    WHERE pc.paper_id = ?1 ORDER BY c.name";
/// Add a paper to a collection (idempotent).
pub const COLLECTION_ADD_PAPER: &str =
    "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) VALUES (?1, ?2)";
/// Remove a paper from a collection.
pub const COLLECTION_REMOVE_PAPER: &str =
    "DELETE FROM paper_collections WHERE paper_id = ?1 AND collection_id = ?2";

/// Look up a tag ID by its name.
pub const TAG_FIND_BY_NAME: &str = "SELECT id FROM tags_live WHERE name = ?1";
/// Insert a new tag.
pub const TAG_INSERT: &str = "INSERT INTO tags (id, name, color) VALUES (?1, ?2, ?3)";
/// List all tags sorted alphabetically.
pub const TAG_LIST: &str = "SELECT id, name, color FROM tags_live ORDER BY name";
/// Associate a tag with a paper (idempotent).
pub const TAG_ADD_TO_PAPER: &str =
    "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)";
/// Rename a tag.
pub const TAG_RENAME: &str = "UPDATE tags SET name = ?1 WHERE id = ?2";
/// Update a tag's color.
pub const TAG_UPDATE_COLOR: &str = "UPDATE tags SET color = ?1 WHERE id = ?2";
/// List tags that have no color assigned.
pub const TAG_LIST_NULL_COLOR: &str = "SELECT id, name FROM tags_live WHERE color IS NULL";
/// List paper IDs associated with a tag.
pub const TAG_PAPER_IDS: &str = "SELECT paper_id FROM paper_tags_live WHERE tag_id = ?1";
/// List the tags applied to a single paper, ordered by name.
pub const TAG_LIST_FOR_PAPER: &str = "\
    SELECT t.id, t.name, t.color FROM tags_live t \
    JOIN paper_tags_live pt ON pt.tag_id = t.id \
    WHERE pt.paper_id = ?1 ORDER BY t.name";
/// Remove a tag association from a paper.
pub const TAG_REMOVE_FROM_PAPER: &str =
    "DELETE FROM paper_tags WHERE paper_id = ?1 AND tag_id = ?2";
/// Delete a tag by ID.
pub const TAG_DELETE: &str = "DELETE FROM tags WHERE id = ?1";

/// Insert a new annotation.
pub const ANNOTATION_INSERT: &str = "\
    INSERT INTO annotations (id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

/// List all annotations for a paper, ordered by page then creation time.
pub const ANNOTATION_LIST_FOR_PAPER: &str = "\
    SELECT id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at \
    FROM annotations_live WHERE paper_id = ?1 ORDER BY page, created_at";

/// Update an annotation's text content.
pub const ANNOTATION_UPDATE_CONTENT: &str =
    "UPDATE annotations SET content = ?1, modified_at = ?2 WHERE id = ?3";
/// Update an annotation's color.
pub const ANNOTATION_UPDATE_COLOR: &str =
    "UPDATE annotations SET color = ?1, modified_at = ?2 WHERE id = ?3";
/// Delete an annotation by ID.
pub const ANNOTATION_DELETE: &str = "DELETE FROM annotations WHERE id = ?1";

/// Insert a new note.
pub const NOTE_INSERT: &str = "\
    INSERT INTO notes (id, paper_id, title, body, created_at, modified_at) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

/// List all notes for a paper, newest first.
pub const NOTE_LIST_FOR_PAPER: &str = "\
    SELECT id, paper_id, title, body, created_at, modified_at \
    FROM notes_live WHERE paper_id = ?1 ORDER BY created_at DESC";

/// Update a note's title and body.
pub const NOTE_UPDATE: &str =
    "UPDATE notes SET title = ?1, body = ?2, modified_at = ?3 WHERE id = ?4";
/// Delete a note by ID.
pub const NOTE_DELETE: &str = "DELETE FROM notes WHERE id = ?1";

/// Insert a new saved search.
pub const SAVED_SEARCH_INSERT: &str =
    "INSERT INTO saved_searches (id, name, query, created_at) VALUES (?1, ?2, ?3, ?4)";

/// List all saved searches, newest first.
pub const SAVED_SEARCH_LIST: &str = "\
    SELECT id, name, query, created_at FROM saved_searches_live ORDER BY created_at DESC";

/// Delete a saved search by ID.
pub const SAVED_SEARCH_DELETE: &str = "DELETE FROM saved_searches WHERE id = ?1";
/// Rename a saved search.
pub const SAVED_SEARCH_RENAME: &str = "UPDATE saved_searches SET name = ?1 WHERE id = ?2";

/// Fetch all paper-tag associations (for graph/export).
pub const GRAPH_ALL_PAPER_TAGS: &str = "SELECT paper_id, tag_id FROM paper_tags_live";
/// Fetch all paper-collection associations (for graph/export).
pub const GRAPH_ALL_PAPER_COLLECTIONS: &str =
    "SELECT paper_id, collection_id FROM paper_collections_live";
/// Fetch all directed citation edges (citing → cited) for the graph.
pub const GRAPH_ALL_CITATIONS: &str = "SELECT citing_paper_id, cited_paper_id FROM paper_citations_live";
/// Upsert one citation edge; ignore if it already exists.
pub const CITATION_INSERT: &str =
    "INSERT OR IGNORE INTO paper_citations (citing_paper_id, cited_paper_id) VALUES (?1, ?2)";

/// Fetch a single paper by ID.
pub const PAPER_GET_BY_ID: &str = "SELECT {COLS} FROM papers_live WHERE id = ?1";

/// Fetch papers with pagination, newest first.
pub const PAPER_LIST_PAGINATED: &str = "\
    SELECT {COLS} FROM papers_live ORDER BY date_added DESC LIMIT ?1 OFFSET ?2";

/// Count unread papers.
pub const PAPER_COUNT_UNREAD: &str = "SELECT COUNT(*) FROM papers_live WHERE is_read = 0";
/// Count favorite papers.
pub const PAPER_COUNT_FAVORITES: &str = "SELECT COUNT(*) FROM papers_live WHERE is_favorite = 1";
/// Count total collections.
pub const COLLECTION_COUNT: &str = "SELECT COUNT(*) FROM collections_live";
/// Count total tags.
pub const TAG_COUNT: &str = "SELECT COUNT(*) FROM tags_live";

// ── Sync bookkeeping ────────────────────────────────────────────────
//
// The statements the sync engine runs that name one fixed table. The ones it
// builds per synced table live in `rotero_db::sync_sql`, which needs the table
// manifest to generate them.

/// This device's sync identity, as lowercase hex.
pub const DEVICE_ID_SELECT: &str = "SELECT lower(hex(site_id)) FROM crr_site_id LIMIT 1";
/// Create the device identity table if a library predates it.
pub const DEVICE_ID_CREATE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS crr_site_id (site_id BLOB PRIMARY KEY)";
/// Seed a device identity, leaving an existing one alone.
pub const DEVICE_ID_SEED: &str =
    "INSERT OR IGNORE INTO crr_site_id (site_id) VALUES (randomblob(16))";
/// Whether the library records a device identity at all.
pub const DEVICE_ID_EXISTS: &str = "SELECT site_id FROM crr_site_id LIMIT 1";

/// Paper IDs a collection holds, including tombstoned memberships.
///
/// Reads the base table rather than the `_live` view: the cascade that deletes a
/// collection needs every membership it must tombstone, dead ones included.
pub const COLLECTION_MEMBER_PAPER_IDS: &str =
    "SELECT paper_id FROM paper_collections WHERE collection_id = ?1";
/// Paper IDs a tag is applied to, including tombstoned associations.
pub const TAG_MEMBER_PAPER_IDS: &str = "SELECT paper_id FROM paper_tags WHERE tag_id = ?1";
/// Collection IDs a paper belongs to, tombstones included.
pub const PAPER_COLLECTION_IDS: &str =
    "SELECT collection_id FROM paper_collections WHERE paper_id = ?1";
/// Tag IDs applied to a paper, tombstones included.
pub const PAPER_TAG_IDS: &str = "SELECT tag_id FROM paper_tags WHERE paper_id = ?1";

/// Another tag holding a name, for reconciling a same-name collision on merge.
pub const TAG_FIND_NAME_CLASH: &str =
    "SELECT id FROM tags WHERE name = ?1 AND id <> ?2 LIMIT 1";
/// Move a retired tag's memberships onto the surviving tag.
pub const TAG_REPOINT_MEMBERSHIPS: &str = "\
    INSERT INTO paper_tags (paper_id, tag_id, updated_at, updated_by, deleted) \
    SELECT paper_id, ?1, ?2, ?3, 0 FROM paper_tags WHERE tag_id = ?4 AND deleted = 0 \
    ON CONFLICT(paper_id, tag_id) DO UPDATE SET deleted = 0, updated_at = ?2";
/// Tombstone every membership of a retired tag.
pub const TAG_TOMBSTONE_MEMBERSHIPS: &str = "\
    UPDATE paper_tags SET deleted = 1, updated_at = ?1, updated_by = ?2 WHERE tag_id = ?3";
/// Retire a duplicate tag, freeing its name.
///
/// The name is cleared, not just tombstoned: `tags.name` is UNIQUE across dead
/// rows too, so leaving it would reject the survivor on every later merge.
pub const TAG_RETIRE_DUPLICATE: &str = "\
    UPDATE tags SET name = ?4, deleted = 1, updated_at = ?1, updated_by = ?2 WHERE id = ?3";

/// Whether a table exists.
pub const TABLE_EXISTS: &str =
    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1";

/// Read a one-time-task flag.
pub const APP_FLAG_SELECT: &str = "SELECT value FROM app_flags WHERE key = ?1";
/// Set a one-time-task flag.
pub const APP_FLAG_UPSERT: &str = "\
    INSERT INTO app_flags (key, value) VALUES (?1, ?2) \
    ON CONFLICT(key) DO UPDATE SET value = ?2";

/// Composite keys of a paper's outgoing citation edges.
pub const PAPER_CITATION_PKS_OUT: &str = "\
    SELECT citing_paper_id || ':' || cited_paper_id FROM paper_citations \
    WHERE citing_paper_id = ?1";
/// Composite keys of a paper's incoming citation edges.
pub const PAPER_CITATION_PKS_IN: &str = "\
    SELECT citing_paper_id || ':' || cited_paper_id FROM paper_citations \
    WHERE cited_paper_id = ?1";

// ── Templates ───────────────────────────────────────────────────────
//
// Statements whose text depends on a value known only at runtime — the shared
// paper column list, or a placeholder run sized to the caller's slice. Kept as
// functions beside the constants so every statement is still in this file.

/// Papers ordered newest first, one page at a time.
pub fn paper_list_paginated() -> String {
    format!("SELECT {PAPER_SELECT_COLS} FROM papers_live ORDER BY date_added DESC LIMIT ?1 OFFSET ?2")
}

/// Papers by id, for `placeholders` bound ids.
pub fn paper_select_by_ids(placeholders: &str) -> String {
    format!("SELECT {PAPER_SELECT_COLS} FROM papers_live WHERE id IN ({placeholders})")
}

/// Distinct paper ids across several collections.
pub fn collection_paper_ids_in(placeholders: &str) -> String {
    format!(
        "SELECT DISTINCT paper_id FROM paper_collections_live \
         WHERE collection_id IN ({placeholders})"
    )
}

/// Ids of a paper's child rows in a table keyed by `column`.
///
/// Reads the base table: the delete cascade needs every child it must
/// tombstone, dead ones included.
pub fn child_ids(table: &str, column: &str) -> String {
    format!("SELECT id FROM {table} WHERE {column} = ?1")
}

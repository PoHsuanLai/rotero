/// SELECT columns for paper queries.
pub const PAPER_SELECT_COLS: &str = "id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key, pdf_url";

/// Insert a new paper row with all columns.
pub const PAPER_INSERT: &str = "\
    INSERT INTO papers (id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key, pdf_url) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)";

/// Count total papers in the library.
pub const PAPER_COUNT: &str = "SELECT COUNT(*) FROM papers";

/// Full-text search across paper metadata and body text, ranked by score.
pub const PAPER_SEARCH_FTS: &str = "\
    SELECT {COLS}, fts_score(title, authors, abstract_text, journal, fulltext, ?1) AS score \
    FROM papers \
    WHERE (title, authors, abstract_text, journal, fulltext) MATCH ?1 OR doi = ?1 \
    ORDER BY score DESC \
    LIMIT 50";

/// Fallback LIKE-based search when FTS is unavailable.
pub const PAPER_SEARCH_LIKE: &str = "\
    SELECT {COLS} FROM papers \
    WHERE title LIKE ?1 OR authors LIKE ?1 OR abstract_text LIKE ?1 OR journal LIKE ?1 OR doi LIKE ?1 OR fulltext LIKE ?1 \
    ORDER BY date_added DESC LIMIT 50";

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
    date_modified = ?12 WHERE id = ?13";

/// Set or replace the local PDF file path for a paper.
pub const PAPER_UPDATE_PDF_PATH: &str =
    "UPDATE papers SET pdf_path = ?1, date_modified = ?2 WHERE id = ?3";
/// Bump the date_modified timestamp on a paper.
pub const PAPER_TOUCH: &str = "UPDATE papers SET date_modified = ?1 WHERE id = ?2";
/// Delete a paper by ID.
pub const PAPER_DELETE: &str = "DELETE FROM papers WHERE id = ?1";

/// Find all papers that share a DOI with at least one other paper.
pub const PAPER_FIND_DOI_DUPLICATES: &str = "\
    SELECT {COLS} FROM papers WHERE doi IS NOT NULL AND doi != '' \
    AND doi IN (SELECT doi FROM papers WHERE doi IS NOT NULL AND doi != '' GROUP BY doi HAVING COUNT(*) > 1) \
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
    SELECT id, doi FROM papers WHERE doi IS NOT NULL AND citation_count IS NULL";

/// Set the citation count for a paper.
pub const PAPER_UPDATE_CITATION_COUNT: &str = "UPDATE papers SET citation_count = ?1 WHERE id = ?2";
/// Set the citation key for a paper.
pub const PAPER_UPDATE_CITATION_KEY: &str = "UPDATE papers SET citation_key = ?1 WHERE id = ?2";

/// List papers that still need an auto-generated citation key.
pub const PAPER_LIST_NEEDING_CITATION_KEYS: &str = "\
    SELECT id, title, authors, year FROM papers \
    WHERE citation_key IS NULL AND title != '' AND title != 'Untitled'";

/// List all existing citation keys (for uniqueness checks).
pub const PAPER_LIST_CITATION_KEYS: &str =
    "SELECT citation_key FROM papers WHERE citation_key IS NOT NULL";

/// List papers that have a remote PDF URL.
pub const PAPER_SELECT_PDF_URL: &str = "SELECT id, pdf_url FROM papers WHERE pdf_url IS NOT NULL";

/// Insert a new collection.
pub const COLLECTION_INSERT: &str =
    "INSERT INTO collections (id, name, parent_id, position) VALUES (?1, ?2, ?3, ?4)";

/// List all collections ordered by hierarchy and position.
pub const COLLECTION_LIST: &str = "\
    SELECT id, name, parent_id, position FROM collections ORDER BY parent_id NULLS FIRST, position";

/// Rename a collection.
pub const COLLECTION_RENAME: &str = "UPDATE collections SET name = ?1 WHERE id = ?2";
/// Move a collection under a new parent.
pub const COLLECTION_REPARENT: &str = "UPDATE collections SET parent_id = ?1 WHERE id = ?2";
/// Delete a collection by ID.
pub const COLLECTION_DELETE: &str = "DELETE FROM collections WHERE id = ?1";

/// List paper IDs belonging to a collection.
pub const COLLECTION_PAPER_IDS: &str =
    "SELECT paper_id FROM paper_collections WHERE collection_id = ?1";
/// Add a paper to a collection (idempotent).
pub const COLLECTION_ADD_PAPER: &str =
    "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) VALUES (?1, ?2)";
/// Remove a paper from a collection.
pub const COLLECTION_REMOVE_PAPER: &str =
    "DELETE FROM paper_collections WHERE paper_id = ?1 AND collection_id = ?2";

/// Look up a tag ID by its name.
pub const TAG_FIND_BY_NAME: &str = "SELECT id FROM tags WHERE name = ?1";
/// Insert a new tag.
pub const TAG_INSERT: &str = "INSERT INTO tags (id, name, color) VALUES (?1, ?2, ?3)";
/// List all tags sorted alphabetically.
pub const TAG_LIST: &str = "SELECT id, name, color FROM tags ORDER BY name";
/// Associate a tag with a paper (idempotent).
pub const TAG_ADD_TO_PAPER: &str =
    "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) VALUES (?1, ?2)";
/// Rename a tag.
pub const TAG_RENAME: &str = "UPDATE tags SET name = ?1 WHERE id = ?2";
/// Update a tag's color.
pub const TAG_UPDATE_COLOR: &str = "UPDATE tags SET color = ?1 WHERE id = ?2";
/// List tags that have no color assigned.
pub const TAG_LIST_NULL_COLOR: &str = "SELECT id, name FROM tags WHERE color IS NULL";
/// List paper IDs associated with a tag.
pub const TAG_PAPER_IDS: &str = "SELECT paper_id FROM paper_tags WHERE tag_id = ?1";
/// Delete a tag by ID.
pub const TAG_DELETE: &str = "DELETE FROM tags WHERE id = ?1";

/// Insert a new annotation.
pub const ANNOTATION_INSERT: &str = "\
    INSERT INTO annotations (id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at) \
    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)";

/// List all annotations for a paper, ordered by page then creation time.
pub const ANNOTATION_LIST_FOR_PAPER: &str = "\
    SELECT id, paper_id, page, ann_type, color, content, geometry, created_at, modified_at \
    FROM annotations WHERE paper_id = ?1 ORDER BY page, created_at";

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
    FROM notes WHERE paper_id = ?1 ORDER BY created_at DESC";

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
    SELECT id, name, query, created_at FROM saved_searches ORDER BY created_at DESC";

/// Delete a saved search by ID.
pub const SAVED_SEARCH_DELETE: &str = "DELETE FROM saved_searches WHERE id = ?1";
/// Rename a saved search.
pub const SAVED_SEARCH_RENAME: &str = "UPDATE saved_searches SET name = ?1 WHERE id = ?2";

/// Fetch all paper-tag associations (for graph/export).
pub const GRAPH_ALL_PAPER_TAGS: &str = "SELECT paper_id, tag_id FROM paper_tags";
/// Fetch all paper-collection associations (for graph/export).
pub const GRAPH_ALL_PAPER_COLLECTIONS: &str =
    "SELECT paper_id, collection_id FROM paper_collections";

/// Fetch a single paper by ID.
pub const PAPER_GET_BY_ID: &str = "SELECT {COLS} FROM papers WHERE id = ?1";

/// Fetch papers with pagination, newest first.
pub const PAPER_LIST_PAGINATED: &str = "\
    SELECT {COLS} FROM papers ORDER BY date_added DESC LIMIT ?1 OFFSET ?2";

/// Count unread papers.
pub const PAPER_COUNT_UNREAD: &str = "SELECT COUNT(*) FROM papers WHERE is_read = 0";
/// Count favorite papers.
pub const PAPER_COUNT_FAVORITES: &str = "SELECT COUNT(*) FROM papers WHERE is_favorite = 1";
/// Count total collections.
pub const COLLECTION_COUNT: &str = "SELECT COUNT(*) FROM collections";
/// Count total tags.
pub const TAG_COUNT: &str = "SELECT COUNT(*) FROM tags";

use chrono::Utc;
use rotero_models::Paper;
use turso::{Connection, Value};

const SELECT_COLS: &str = "id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key";

pub async fn insert_paper(conn: &Connection, paper: &Paper) -> Result<i64, turso::Error> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    let extra_meta = paper
        .extra_meta
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO papers (title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta, citation_count, citation_key)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        turso::params::Params::Positional(vec![
            Value::Text(paper.title.clone()),
            Value::Text(authors_json),
            paper.year.map(|y| Value::Integer(y as i64)).unwrap_or(Value::Null),
            paper.doi.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.abstract_text.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.journal.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.volume.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.issue.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.pages.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.publisher.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.url.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            paper.pdf_path.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
            Value::Text(paper.date_added.to_rfc3339()),
            Value::Text(paper.date_modified.to_rfc3339()),
            Value::Integer(paper.is_favorite as i64),
            Value::Integer(paper.is_read as i64),
            extra_meta.map(Value::Text).unwrap_or(Value::Null),
            paper.citation_count.map(Value::Integer).unwrap_or(Value::Null),
            paper.citation_key.as_ref().map(|s| Value::Text(s.clone())).unwrap_or(Value::Null),
        ]),
    )
    .await?;

    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_papers(conn: &Connection) -> Result<Vec<Paper>, turso::Error> {
    list_papers_paginated(conn, 0, 500).await
}

/// Load papers with pagination support.
pub async fn list_papers_paginated(
    conn: &Connection,
    offset: u32,
    limit: u32,
) -> Result<Vec<Paper>, turso::Error> {
    let sql =
        format!("SELECT {SELECT_COLS} FROM papers ORDER BY date_added DESC LIMIT ?1 OFFSET ?2");
    let mut rows = conn
        .query(
            &sql,
            [Value::Integer(limit as i64), Value::Integer(offset as i64)],
        )
        .await?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

/// Return total number of papers in the library.
#[allow(dead_code)]
pub async fn count_papers(conn: &Connection) -> Result<u32, turso::Error> {
    let mut rows = conn.query("SELECT COUNT(*) FROM papers", ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
}

pub async fn search_papers(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
    // Try FTS first, fall back to LIKE if FTS is unavailable or query fails
    match search_papers_fts(conn, query).await {
        Ok(results) => Ok(results),
        Err(_) => search_papers_like(conn, query).await,
    }
}

async fn search_papers_fts(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
    let sql = format!(
        "SELECT {SELECT_COLS}, fts_score(title, authors, abstract_text, journal, fulltext, ?1) AS score \
         FROM papers \
         WHERE (title, authors, abstract_text, journal, fulltext) MATCH ?1 OR doi = ?1 \
         ORDER BY score DESC \
         LIMIT 50"
    );
    let mut rows = conn.query(&sql, [Value::Text(query.to_string())]).await?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

async fn search_papers_like(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
    let pattern = format!("%{query}%");
    let sql = format!(
        "SELECT {SELECT_COLS} FROM papers WHERE title LIKE ?1 OR authors LIKE ?1 OR abstract_text LIKE ?1 OR journal LIKE ?1 OR doi LIKE ?1 OR fulltext LIKE ?1 ORDER BY date_added DESC LIMIT 50"
    );
    let mut rows = conn.query(&sql, [Value::Text(pattern)]).await?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

pub async fn set_favorite(conn: &Connection, id: i64, favorite: bool) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET is_favorite = ?1 WHERE id = ?2",
        [Value::Integer(favorite as i64), Value::Integer(id)],
    )
    .await?;
    Ok(())
}

pub async fn set_read(conn: &Connection, id: i64, read: bool) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET is_read = ?1 WHERE id = ?2",
        [Value::Integer(read as i64), Value::Integer(id)],
    )
    .await?;
    Ok(())
}

/// Store extracted PDF body text for full-text search.
pub async fn update_paper_fulltext(
    conn: &Connection,
    id: i64,
    text: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET fulltext = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![Value::Text(text.to_string()), Value::Integer(id)]),
    )
    .await?;
    Ok(())
}

/// Update all metadata fields for an existing paper.
pub async fn update_paper_metadata(
    conn: &Connection,
    id: i64,
    paper: &Paper,
) -> Result<(), turso::Error> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE papers SET title = ?1, authors = ?2, year = ?3, doi = ?4, abstract_text = ?5,
         journal = ?6, volume = ?7, issue = ?8, pages = ?9, publisher = ?10, url = ?11,
         date_modified = ?12 WHERE id = ?13",
        turso::params::Params::Positional(vec![
            Value::Text(paper.title.clone()),
            Value::Text(authors_json),
            paper
                .year
                .map(|y| Value::Integer(y as i64))
                .unwrap_or(Value::Null),
            paper
                .doi
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .abstract_text
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .journal
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .volume
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .issue
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .pages
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publisher
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .url
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            Value::Text(Utc::now().to_rfc3339()),
            Value::Integer(id),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn update_pdf_path(
    conn: &Connection,
    id: i64,
    pdf_path: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET pdf_path = ?1, date_modified = ?2 WHERE id = ?3",
        turso::params::Params::Positional(vec![
            Value::Text(pdf_path.to_string()),
            Value::Text(chrono::Utc::now().to_rfc3339()),
            Value::Integer(id),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn delete_paper(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM papers WHERE id = ?1", [id])
        .await?;
    Ok(())
}

fn get_text(row: &turso::Row, idx: usize) -> String {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default()
}

fn get_opt_text(row: &turso::Row, idx: usize) -> Option<String> {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned())
}

fn get_opt_i64(row: &turso::Row, idx: usize) -> Option<i64> {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_integer().copied())
}

fn get_bool(row: &turso::Row, idx: usize) -> bool {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_integer().copied())
        .unwrap_or(0)
        != 0
}

fn row_to_paper(row: &turso::Row) -> Paper {
    let authors_str = get_text(row, 2);
    let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();

    let date_added_str = get_text(row, 13);
    let date_modified_str = get_text(row, 14);
    let extra_meta_str = get_opt_text(row, 17);

    Paper {
        id: get_opt_i64(row, 0),
        title: get_text(row, 1),
        authors,
        year: get_opt_i64(row, 3).map(|i| i as i32),
        doi: get_opt_text(row, 4),
        abstract_text: get_opt_text(row, 5),
        journal: get_opt_text(row, 6),
        volume: get_opt_text(row, 7),
        issue: get_opt_text(row, 8),
        pages: get_opt_text(row, 9),
        publisher: get_opt_text(row, 10),
        url: get_opt_text(row, 11),
        pdf_path: get_opt_text(row, 12),
        date_added: chrono::DateTime::parse_from_rfc3339(&date_added_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        date_modified: chrono::DateTime::parse_from_rfc3339(&date_modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        is_favorite: get_bool(row, 15),
        is_read: get_bool(row, 16),
        citation_count: get_opt_i64(row, 18),
        citation_key: get_opt_text(row, 19),
        extra_meta: extra_meta_str.and_then(|s| serde_json::from_str(&s).ok()),
    }
}

/// Find duplicate papers grouped by DOI or normalized title.
/// Returns groups of 2+ papers that share the same DOI or similar title.
pub async fn find_duplicates(conn: &Connection) -> Result<Vec<Vec<Paper>>, turso::Error> {
    let mut groups: Vec<Vec<Paper>> = Vec::new();

    // Group 1: exact DOI duplicates
    let doi_sql = format!(
        "SELECT {SELECT_COLS} FROM papers WHERE doi IS NOT NULL AND doi != '' \
         AND doi IN (SELECT doi FROM papers WHERE doi IS NOT NULL AND doi != '' GROUP BY doi HAVING COUNT(*) > 1) \
         ORDER BY doi, date_added DESC"
    );
    let mut rows = conn.query(&doi_sql, ()).await?;
    let mut doi_papers: Vec<Paper> = Vec::new();
    while let Some(row) = rows.next().await? {
        doi_papers.push(row_to_paper(&row));
    }
    // Group by DOI
    let mut current_doi = String::new();
    let mut current_group: Vec<Paper> = Vec::new();
    for paper in doi_papers {
        let doi = paper.doi.clone().unwrap_or_default();
        if doi != current_doi && !current_group.is_empty() {
            groups.push(std::mem::take(&mut current_group));
        }
        current_doi = doi;
        current_group.push(paper);
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    // Group 2: normalized title duplicates (excluding papers already found by DOI)
    let doi_ids: Vec<i64> = groups.iter().flatten().filter_map(|p| p.id).collect();
    let all = list_papers(conn).await?;
    let mut title_map: std::collections::HashMap<String, Vec<Paper>> =
        std::collections::HashMap::new();
    for paper in all {
        if doi_ids.contains(&paper.id.unwrap_or(0)) {
            continue;
        }
        let normalized = normalize_title(&paper.title);
        if normalized.is_empty() {
            continue;
        }
        title_map.entry(normalized).or_default().push(paper);
    }
    for (_title, papers) in title_map {
        if papers.len() > 1 {
            groups.push(papers);
        }
    }

    Ok(groups)
}

fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Merge two papers: transfer associations from `delete_id` to `keep_id`, then delete.
pub async fn merge_papers(
    conn: &Connection,
    keep_id: i64,
    delete_id: i64,
) -> Result<(), turso::Error> {
    // Transfer collection memberships
    conn.execute(
        "INSERT OR IGNORE INTO paper_collections (paper_id, collection_id) \
         SELECT ?1, collection_id FROM paper_collections WHERE paper_id = ?2",
        [Value::Integer(keep_id), Value::Integer(delete_id)],
    )
    .await?;
    // Transfer tag assignments
    conn.execute(
        "INSERT OR IGNORE INTO paper_tags (paper_id, tag_id) \
         SELECT ?1, tag_id FROM paper_tags WHERE paper_id = ?2",
        [Value::Integer(keep_id), Value::Integer(delete_id)],
    )
    .await?;
    // Delete the duplicate
    delete_paper(conn, delete_id).await?;
    Ok(())
}

/// Update citation count for a paper.
/// Return (id, doi) pairs for papers that have a DOI but no citation count yet.
pub async fn list_papers_needing_citations(
    conn: &Connection,
) -> Result<Vec<(i64, String)>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, doi FROM papers WHERE doi IS NOT NULL AND citation_count IS NULL",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
        let doi = row.get_value(1)?.as_text().cloned().unwrap_or_default();
        if !doi.is_empty() {
            out.push((id, doi));
        }
    }
    Ok(out)
}

pub async fn update_citation_count(
    conn: &Connection,
    id: i64,
    count: i64,
) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET citation_count = ?1 WHERE id = ?2",
        [Value::Integer(count), Value::Integer(id)],
    )
    .await?;
    Ok(())
}

/// Update the citation key for a paper.
pub async fn update_citation_key(
    conn: &Connection,
    id: i64,
    key: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET citation_key = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![Value::Text(key.to_string()), Value::Integer(id)]),
    )
    .await?;
    Ok(())
}

/// Return (id, title, authors_json, year) for papers that need a citation key generated.
pub async fn list_papers_needing_citation_keys(
    conn: &Connection,
) -> Result<Vec<(i64, String, Vec<String>, Option<i32>)>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, title, authors, year FROM papers \
             WHERE citation_key IS NULL AND title != '' AND title != 'Untitled'",
            (),
        )
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
        let title = row
            .get_value(1)
            .ok()
            .and_then(|v| v.as_text().cloned())
            .unwrap_or_default();
        let authors_str = row
            .get_value(2)
            .ok()
            .and_then(|v| v.as_text().cloned())
            .unwrap_or_else(|| "[]".to_string());
        let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();
        let year = row
            .get_value(3)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .map(|y| y as i32);
        out.push((id, title, authors, year));
    }
    Ok(out)
}

/// List all existing citation keys (for deduplication).
pub async fn list_citation_keys(conn: &Connection) -> Result<Vec<String>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT citation_key FROM papers WHERE citation_key IS NOT NULL",
            (),
        )
        .await?;
    let mut keys = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(key) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
            keys.push(key);
        }
    }
    Ok(keys)
}

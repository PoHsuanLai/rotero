use chrono::Utc;
use rotero_models::{CitationInfo, LibraryStatus, Paper, PaperLinks, Publication};
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

pub async fn insert_paper(conn: &Connection, paper: &Paper) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    let extra_meta = paper
        .citation
        .extra_meta
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        queries::PAPER_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
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
                .publication
                .journal
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .volume
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .issue
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .pages
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .publisher
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .links
                .url
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .links
                .pdf_path
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            Value::Text(paper.status.date_added.to_rfc3339()),
            Value::Text(paper.status.date_modified.to_rfc3339()),
            Value::Integer(paper.status.is_favorite as i64),
            Value::Integer(paper.status.is_read as i64),
            extra_meta.map(Value::Text).unwrap_or(Value::Null),
            paper
                .citation
                .citation_count
                .map(Value::Integer)
                .unwrap_or(Value::Null),
            paper
                .citation
                .citation_key
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .links
                .pdf_url
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
        ]),
    )
    .await?;

    crr::track_insert(
        conn,
        "papers",
        &uuid,
        &[
            "title",
            "authors",
            "year",
            "doi",
            "abstract_text",
            "journal",
            "volume",
            "issue",
            "pages",
            "publisher",
            "url",
            "pdf_path",
            "date_added",
            "date_modified",
            "is_favorite",
            "is_read",
            "extra_meta",
            "citation_count",
            "citation_key",
            "pdf_url",
        ],
    )
    .await?;

    Ok(uuid)
}

pub async fn list_papers(conn: &Connection) -> Result<Vec<Paper>, turso::Error> {
    list_papers_paginated(conn, 0, 500).await
}

pub async fn list_papers_paginated(
    conn: &Connection,
    offset: u32,
    limit: u32,
) -> Result<Vec<Paper>, turso::Error> {
    let sql = format!(
        "SELECT {} FROM papers ORDER BY date_added DESC LIMIT ?1 OFFSET ?2",
        queries::PAPER_SELECT_COLS
    );
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

pub async fn count_papers(conn: &Connection) -> Result<u32, turso::Error> {
    let mut rows = conn.query(queries::PAPER_COUNT, ()).await?;
    let row = rows
        .next()
        .await?
        .ok_or(turso::Error::QueryReturnedNoRows)?;
    Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0) as u32)
}

pub async fn search_papers(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
    // FTS first, fall back to LIKE if unavailable
    match search_papers_fts(conn, query).await {
        Ok(results) => Ok(results),
        Err(_) => search_papers_like(conn, query).await,
    }
}

async fn search_papers_fts(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
    let sql = queries::PAPER_SEARCH_FTS.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn.query(&sql, [Value::Text(query.to_string())]).await?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

async fn search_papers_like(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
    let pattern = format!("%{query}%");
    let sql = queries::PAPER_SEARCH_LIKE.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn.query(&sql, [Value::Text(pattern)]).await?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

pub async fn set_favorite(conn: &Connection, id: &str, favorite: bool) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_SET_FAVORITE,
        [Value::Integer(favorite as i64), Value::Text(id.to_string())],
    )
    .await?;
    crr::track_update(conn, "papers", id, &["is_favorite"]).await?;
    Ok(())
}

pub async fn set_read(conn: &Connection, id: &str, read: bool) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_SET_READ,
        [Value::Integer(read as i64), Value::Text(id.to_string())],
    )
    .await?;
    crr::track_update(conn, "papers", id, &["is_read"]).await?;
    Ok(())
}

pub async fn update_paper_fulltext(
    conn: &Connection,
    id: &str,
    text: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_UPDATE_FULLTEXT,
        turso::params::Params::Positional(vec![
            Value::Text(text.to_string()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    Ok(())
}

pub async fn update_paper_metadata(
    conn: &Connection,
    id: &str,
    paper: &Paper,
) -> Result<(), turso::Error> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        queries::PAPER_UPDATE_METADATA,
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
                .publication
                .journal
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .volume
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .issue
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .pages
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .publication
                .publisher
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            paper
                .links
                .url
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            Value::Text(Utc::now().to_rfc3339()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(
        conn,
        "papers",
        id,
        &[
            "title",
            "authors",
            "year",
            "doi",
            "abstract_text",
            "journal",
            "volume",
            "issue",
            "pages",
            "publisher",
            "url",
            "date_modified",
        ],
    )
    .await?;
    Ok(())
}

pub async fn update_pdf_path(
    conn: &Connection,
    id: &str,
    pdf_path: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_UPDATE_PDF_PATH,
        turso::params::Params::Positional(vec![
            Value::Text(pdf_path.to_string()),
            Value::Text(chrono::Utc::now().to_rfc3339()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(conn, "papers", id, &["pdf_path", "date_modified"]).await?;
    Ok(())
}

pub async fn touch_paper(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        queries::PAPER_TOUCH,
        [Value::Text(now), Value::Text(id.to_string())],
    )
    .await?;
    crr::track_update(conn, "papers", id, &["date_modified"]).await?;
    Ok(())
}

pub async fn delete_paper(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::PAPER_DELETE, [Value::Text(id.to_string())])
        .await?;
    crr::track_delete(conn, "papers", id).await?;
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
        id: get_opt_text(row, 0),
        title: get_text(row, 1),
        authors,
        year: get_opt_i64(row, 3).map(|i| i as i32),
        doi: get_opt_text(row, 4),
        abstract_text: get_opt_text(row, 5),
        publication: Publication {
            journal: get_opt_text(row, 6),
            volume: get_opt_text(row, 7),
            issue: get_opt_text(row, 8),
            pages: get_opt_text(row, 9),
            publisher: get_opt_text(row, 10),
        },
        links: PaperLinks {
            url: get_opt_text(row, 11),
            pdf_path: get_opt_text(row, 12),
            pdf_url: get_opt_text(row, 20),
        },
        status: LibraryStatus {
            date_added: chrono::DateTime::parse_from_rfc3339(&date_added_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            date_modified: chrono::DateTime::parse_from_rfc3339(&date_modified_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            is_favorite: get_bool(row, 15),
            is_read: get_bool(row, 16),
        },
        citation: CitationInfo {
            citation_count: get_opt_i64(row, 18),
            citation_key: get_opt_text(row, 19),
            extra_meta: extra_meta_str.and_then(|s| serde_json::from_str(&s).ok()),
        },
    }
}

/// Returns groups of 2+ papers that share the same DOI or normalized title.
pub async fn find_duplicates(conn: &Connection) -> Result<Vec<Vec<Paper>>, turso::Error> {
    let mut groups: Vec<Vec<Paper>> = Vec::new();

    // Exact DOI duplicates
    let doi_sql = queries::PAPER_FIND_DOI_DUPLICATES.replace("{COLS}", queries::PAPER_SELECT_COLS);
    let mut rows = conn.query(&doi_sql, ()).await?;
    let mut doi_papers: Vec<Paper> = Vec::new();
    while let Some(row) = rows.next().await? {
        doi_papers.push(row_to_paper(&row));
    }
    let mut current_doi = String::new();
    let mut current_group: Vec<Paper> = Vec::new();
    for paper in doi_papers {
        let doi = paper.doi.as_deref().unwrap_or_default();
        if doi != current_doi.as_str() && !current_group.is_empty() {
            groups.push(std::mem::take(&mut current_group));
        }
        current_doi = doi.to_string();
        current_group.push(paper);
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    // Normalized title duplicates (excluding papers already found by DOI)
    let doi_ids: std::collections::HashSet<String> = groups
        .iter()
        .flatten()
        .filter_map(|p| p.id.clone())
        .collect();
    let all = list_papers(conn).await?;
    let mut title_map: std::collections::HashMap<String, Vec<Paper>> =
        std::collections::HashMap::new();
    for paper in all {
        if paper.id.as_ref().is_some_and(|id| doi_ids.contains(id)) {
            continue;
        }
        let normalized = normalize_title(&paper.title);
        if normalized.is_empty() {
            continue;
        }
        title_map.entry(normalized).or_default().push(paper);
    }
    for papers in title_map.into_values() {
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

/// Transfer associations from `delete_id` to `keep_id`, then delete the duplicate.
pub async fn merge_papers(
    conn: &Connection,
    keep_id: &str,
    delete_id: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_MERGE_COLLECTIONS,
        [
            Value::Text(keep_id.to_string()),
            Value::Text(delete_id.to_string()),
        ],
    )
    .await?;
    conn.execute(
        queries::PAPER_MERGE_TAGS,
        [
            Value::Text(keep_id.to_string()),
            Value::Text(delete_id.to_string()),
        ],
    )
    .await?;
    delete_paper(conn, delete_id).await?;
    Ok(())
}

/// Return (id, doi) pairs for papers that have a DOI but no citation count yet.
pub async fn list_papers_needing_citations(
    conn: &Connection,
) -> Result<Vec<(String, String)>, turso::Error> {
    let mut rows = conn
        .query(queries::PAPER_LIST_NEEDING_CITATIONS, ())
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
        let doi = row.get_value(1)?.as_text().cloned().unwrap_or_default();
        if !doi.is_empty() {
            out.push((id, doi));
        }
    }
    Ok(out)
}

pub async fn update_citation_count(
    conn: &Connection,
    id: &str,
    count: i64,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_UPDATE_CITATION_COUNT,
        [Value::Integer(count), Value::Text(id.to_string())],
    )
    .await?;
    crr::track_update(conn, "papers", id, &["citation_count"]).await?;
    Ok(())
}

pub async fn update_citation_key(
    conn: &Connection,
    id: &str,
    key: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::PAPER_UPDATE_CITATION_KEY,
        turso::params::Params::Positional(vec![
            Value::Text(key.to_string()),
            Value::Text(id.to_string()),
        ]),
    )
    .await?;
    crr::track_update(conn, "papers", id, &["citation_key"]).await?;
    Ok(())
}

/// Return (id, title, authors, year) for papers missing a citation key.
pub async fn list_papers_needing_citation_keys(
    conn: &Connection,
) -> Result<Vec<(String, String, Vec<String>, Option<i32>)>, turso::Error> {
    let mut rows = conn
        .query(queries::PAPER_LIST_NEEDING_CITATION_KEYS, ())
        .await?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        let id = row.get_value(0)?.as_text().cloned().unwrap_or_default();
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

/// List all existing citation keys (for dedup when generating new ones).
pub async fn list_citation_keys(conn: &Connection) -> Result<Vec<String>, turso::Error> {
    let mut rows = conn.query(queries::PAPER_LIST_CITATION_KEYS, ()).await?;
    let mut keys = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(key) = row.get_value(0).ok().and_then(|v| v.as_text().cloned()) {
            keys.push(key);
        }
    }
    Ok(keys)
}

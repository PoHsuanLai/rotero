use chrono::Utc;
use turso::{Connection, Value};
use rotero_models::Paper;

const SELECT_COLS: &str = "id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta";

pub async fn insert_paper(conn: &Connection, paper: &Paper) -> Result<i64, turso::Error> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    let extra_meta = paper
        .extra_meta
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO papers (title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, is_favorite, is_read, extra_meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
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
            extra_meta.map(|s| Value::Text(s)).unwrap_or(Value::Null),
        ]),
    )
    .await?;

    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows.next().await?.ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_papers(conn: &Connection) -> Result<Vec<Paper>, turso::Error> {
    let sql = format!("SELECT {SELECT_COLS} FROM papers ORDER BY date_added DESC");
    let mut rows = conn.query(&sql, ()).await?;
    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

pub async fn search_papers(conn: &Connection, query: &str) -> Result<Vec<Paper>, turso::Error> {
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
    ).await?;
    Ok(())
}

pub async fn set_read(conn: &Connection, id: i64, read: bool) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET is_read = ?1 WHERE id = ?2",
        [Value::Integer(read as i64), Value::Integer(id)],
    ).await?;
    Ok(())
}

/// Store extracted PDF body text for full-text search.
pub async fn update_paper_fulltext(conn: &Connection, id: i64, text: &str) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE papers SET fulltext = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![Value::Text(text.to_string()), Value::Integer(id)]),
    ).await?;
    Ok(())
}

/// Update all metadata fields for an existing paper.
pub async fn update_paper_metadata(conn: &Connection, id: i64, paper: &Paper) -> Result<(), turso::Error> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE papers SET title = ?1, authors = ?2, year = ?3, doi = ?4, abstract_text = ?5,
         journal = ?6, volume = ?7, issue = ?8, pages = ?9, publisher = ?10, url = ?11,
         date_modified = ?12 WHERE id = ?13",
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
            Value::Text(Utc::now().to_rfc3339()),
            Value::Integer(id),
        ]),
    ).await?;
    Ok(())
}

pub async fn delete_paper(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM papers WHERE id = ?1", [id]).await?;
    Ok(())
}

pub async fn count_papers_in_collection(conn: &Connection, collection_id: i64) -> Result<i64, turso::Error> {
    let mut rows = conn.query(
        "SELECT COUNT(*) FROM paper_collections WHERE collection_id = ?1",
        [collection_id],
    ).await?;
    let row = rows.next().await?.ok_or(turso::Error::QueryReturnedNoRows)?;
    let count = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(count)
}

fn get_text(row: &turso::Row, idx: usize) -> String {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned()).unwrap_or_default()
}

fn get_opt_text(row: &turso::Row, idx: usize) -> Option<String> {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned())
}

fn get_opt_i64(row: &turso::Row, idx: usize) -> Option<i64> {
    row.get_value(idx).ok().and_then(|v| v.as_integer().copied())
}

fn get_bool(row: &turso::Row, idx: usize) -> bool {
    row.get_value(idx).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) != 0
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
        extra_meta: extra_meta_str.and_then(|s| serde_json::from_str(&s).ok()),
    }
}

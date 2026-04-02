use chrono::Utc;
use turso::{Connection, Value};
use rotero_models::Paper;

pub async fn insert_paper(conn: &Connection, paper: &Paper) -> Result<i64, turso::Error> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    let extra_meta = paper
        .extra_meta
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO papers (title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, extra_meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
            extra_meta.map(|s| Value::Text(s)).unwrap_or(Value::Null),
        ]),
    )
    .await?;

    // Get last insert rowid
    let mut rows = conn.query("SELECT last_insert_rowid()", ()).await?;
    let row = rows.next().await?.ok_or(turso::Error::QueryReturnedNoRows)?;
    let id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
    Ok(id)
}

pub async fn list_papers(conn: &Connection) -> Result<Vec<Paper>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, extra_meta
             FROM papers ORDER BY date_added DESC",
            (),
        )
        .await?;

    let mut papers = Vec::new();
    while let Some(row) = rows.next().await? {
        papers.push(row_to_paper(&row));
    }
    Ok(papers)
}

pub async fn delete_paper(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM papers WHERE id = ?1", [id]).await?;
    Ok(())
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

fn row_to_paper(row: &turso::Row) -> Paper {
    let authors_str = get_text(row, 2);
    let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();

    let date_added_str = get_text(row, 13);
    let date_modified_str = get_text(row, 14);
    let extra_meta_str = get_opt_text(row, 15);

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
        extra_meta: extra_meta_str.and_then(|s| serde_json::from_str(&s).ok()),
    }
}

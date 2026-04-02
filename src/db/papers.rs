use chrono::Utc;
use rusqlite::{Connection, params};
use rotero_models::Paper;

pub fn insert_paper(conn: &Connection, paper: &Paper) -> rusqlite::Result<i64> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    let extra_meta = paper
        .extra_meta
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());

    conn.execute(
        "INSERT INTO papers (title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, extra_meta)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            paper.title,
            authors_json,
            paper.year,
            paper.doi,
            paper.abstract_text,
            paper.journal,
            paper.volume,
            paper.issue,
            paper.pages,
            paper.publisher,
            paper.url,
            paper.pdf_path,
            paper.date_added.to_rfc3339(),
            paper.date_modified.to_rfc3339(),
            extra_meta,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_paper(conn: &Connection, id: i64) -> rusqlite::Result<Paper> {
    conn.query_row(
        "SELECT id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, extra_meta
         FROM papers WHERE id = ?1",
        [id],
        |row| {
            Ok(row_to_paper(row))
        },
    )
}

pub fn list_papers(conn: &Connection) -> rusqlite::Result<Vec<Paper>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, authors, year, doi, abstract_text, journal, volume, issue, pages, publisher, url, pdf_path, date_added, date_modified, extra_meta
         FROM papers ORDER BY date_added DESC",
    )?;
    let papers = stmt
        .query_map([], |row| Ok(row_to_paper(row)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(papers)
}

pub fn list_papers_in_collection(conn: &Connection, collection_id: i64) -> rusqlite::Result<Vec<Paper>> {
    let mut stmt = conn.prepare(
        "SELECT p.id, p.title, p.authors, p.year, p.doi, p.abstract_text, p.journal, p.volume, p.issue, p.pages, p.publisher, p.url, p.pdf_path, p.date_added, p.date_modified, p.extra_meta
         FROM papers p
         JOIN paper_collections pc ON p.id = pc.paper_id
         WHERE pc.collection_id = ?1
         ORDER BY p.date_added DESC",
    )?;
    let papers = stmt
        .query_map([collection_id], |row| Ok(row_to_paper(row)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(papers)
}

pub fn update_paper(conn: &Connection, paper: &Paper) -> rusqlite::Result<()> {
    let authors_json = serde_json::to_string(&paper.authors).unwrap_or_else(|_| "[]".to_string());
    let extra_meta = paper
        .extra_meta
        .as_ref()
        .map(|v| serde_json::to_string(v).unwrap_or_default());
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "UPDATE papers SET title=?1, authors=?2, year=?3, doi=?4, abstract_text=?5, journal=?6, volume=?7, issue=?8, pages=?9, publisher=?10, url=?11, pdf_path=?12, date_modified=?13, extra_meta=?14
         WHERE id=?15",
        params![
            paper.title,
            authors_json,
            paper.year,
            paper.doi,
            paper.abstract_text,
            paper.journal,
            paper.volume,
            paper.issue,
            paper.pages,
            paper.publisher,
            paper.url,
            paper.pdf_path,
            now,
            extra_meta,
            paper.id,
        ],
    )?;
    Ok(())
}

pub fn delete_paper(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM papers WHERE id = ?1", [id])?;
    Ok(())
}

fn row_to_paper(row: &rusqlite::Row) -> Paper {
    let authors_str: String = row.get(2).unwrap_or_default();
    let authors: Vec<String> = serde_json::from_str(&authors_str).unwrap_or_default();

    let date_added_str: String = row.get(13).unwrap_or_default();
    let date_modified_str: String = row.get(14).unwrap_or_default();
    let extra_meta_str: Option<String> = row.get(15).unwrap_or(None);

    Paper {
        id: row.get(0).ok(),
        title: row.get(1).unwrap_or_default(),
        authors,
        year: row.get(3).unwrap_or(None),
        doi: row.get(4).unwrap_or(None),
        abstract_text: row.get(5).unwrap_or(None),
        journal: row.get(6).unwrap_or(None),
        volume: row.get(7).unwrap_or(None),
        issue: row.get(8).unwrap_or(None),
        pages: row.get(9).unwrap_or(None),
        publisher: row.get(10).unwrap_or(None),
        url: row.get(11).unwrap_or(None),
        pdf_path: row.get(12).unwrap_or(None),
        date_added: chrono::DateTime::parse_from_rfc3339(&date_added_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        date_modified: chrono::DateTime::parse_from_rfc3339(&date_modified_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        extra_meta: extra_meta_str.and_then(|s| serde_json::from_str(&s).ok()),
    }
}

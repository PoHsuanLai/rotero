use chrono::Utc;
use rotero_models::SavedSearch;
use turso::{Connection, Value};

pub async fn insert_saved_search(
    conn: &Connection,
    search: &SavedSearch,
) -> Result<i64, turso::Error> {
    conn.execute(
        "INSERT INTO saved_searches (name, query, created_at) VALUES (?1, ?2, ?3)",
        turso::params::Params::Positional(vec![
            Value::Text(search.name.clone()),
            Value::Text(search.query.clone()),
            Value::Text(search.created_at.to_rfc3339()),
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

pub async fn list_saved_searches(conn: &Connection) -> Result<Vec<SavedSearch>, turso::Error> {
    let mut rows = conn
        .query(
            "SELECT id, name, query, created_at FROM saved_searches ORDER BY created_at DESC",
            (),
        )
        .await?;
    let mut searches = Vec::new();
    while let Some(row) = rows.next().await? {
        searches.push(row_to_saved_search(&row));
    }
    Ok(searches)
}

pub async fn delete_saved_search(conn: &Connection, id: i64) -> Result<(), turso::Error> {
    conn.execute("DELETE FROM saved_searches WHERE id = ?1", [id])
        .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn rename_saved_search(
    conn: &Connection,
    id: i64,
    name: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        "UPDATE saved_searches SET name = ?1 WHERE id = ?2",
        [Value::Text(name.to_string()), Value::Integer(id)],
    )
    .await?;
    Ok(())
}

fn row_to_saved_search(row: &turso::Row) -> SavedSearch {
    let id = row.get_value(0).ok().and_then(|v| v.as_integer().copied());
    let name = row
        .get_value(1)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let query = row
        .get_value(2)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();
    let created_str = row
        .get_value(3)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default();

    SavedSearch {
        id,
        name,
        query,
        created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    }
}

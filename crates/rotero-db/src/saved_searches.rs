use chrono::Utc;
use rotero_models::SavedSearch;
use turso::{Connection, Value};

use crate::crr;
use crate::queries;

pub async fn insert_saved_search(
    conn: &Connection,
    search: &SavedSearch,
) -> Result<String, turso::Error> {
    let uuid = uuid::Uuid::now_v7().to_string();
    conn.execute(
        queries::SAVED_SEARCH_INSERT,
        turso::params::Params::Positional(vec![
            Value::Text(uuid.clone()),
            Value::Text(search.name.clone()),
            Value::Text(search.query.clone()),
            Value::Text(search.created_at.to_rfc3339()),
        ]),
    )
    .await?;

    crr::track_insert(
        conn,
        "saved_searches",
        &uuid,
        &["name", "query", "created_at"],
    )
    .await?;

    Ok(uuid)
}

pub async fn list_saved_searches(conn: &Connection) -> Result<Vec<SavedSearch>, turso::Error> {
    let mut rows = conn.query(queries::SAVED_SEARCH_LIST, ()).await?;
    let mut searches = Vec::new();
    while let Some(row) = rows.next().await? {
        searches.push(row_to_saved_search(&row));
    }
    Ok(searches)
}

pub async fn delete_saved_search(conn: &Connection, id: &str) -> Result<(), turso::Error> {
    conn.execute(queries::SAVED_SEARCH_DELETE, [Value::Text(id.to_string())])
        .await?;
    crr::track_delete(conn, "saved_searches", id).await?;
    Ok(())
}

pub async fn rename_saved_search(
    conn: &Connection,
    id: &str,
    name: &str,
) -> Result<(), turso::Error> {
    conn.execute(
        queries::SAVED_SEARCH_RENAME,
        [Value::Text(name.to_string()), Value::Text(id.to_string())],
    )
    .await?;
    crr::track_update(conn, "saved_searches", id, &["name"]).await?;
    Ok(())
}

fn row_to_saved_search(row: &turso::Row) -> SavedSearch {
    let id = row.get_value(0).ok().and_then(|v| v.as_text().cloned());
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

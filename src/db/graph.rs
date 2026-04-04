use turso::Connection;

use super::queries;

/// Fetch all (paper_id, tag_id) pairs from the paper_tags junction table.
pub async fn list_all_paper_tags(conn: &Connection) -> Result<Vec<(i64, i64)>, turso::Error> {
    let mut rows = conn.query(queries::GRAPH_ALL_PAPER_TAGS, ()).await?;
    let mut pairs = Vec::new();
    while let Some(row) = rows.next().await? {
        let paper_id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
        let tag_id = row.get_value(1)?.as_integer().copied().unwrap_or(0);
        pairs.push((paper_id, tag_id));
    }
    Ok(pairs)
}

/// Fetch all (paper_id, collection_id) pairs from the paper_collections junction table.
pub async fn list_all_paper_collections(
    conn: &Connection,
) -> Result<Vec<(i64, i64)>, turso::Error> {
    let mut rows = conn.query(queries::GRAPH_ALL_PAPER_COLLECTIONS, ()).await?;
    let mut pairs = Vec::new();
    while let Some(row) = rows.next().await? {
        let paper_id = row.get_value(0)?.as_integer().copied().unwrap_or(0);
        let coll_id = row.get_value(1)?.as_integer().copied().unwrap_or(0);
        pairs.push((paper_id, coll_id));
    }
    Ok(pairs)
}

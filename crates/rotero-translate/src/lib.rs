mod node;
mod server;
mod translator;

pub use server::TranslationServer;
pub use translator::ZoteroItem;

use rotero_models::Paper;

#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("Server not running")]
    ServerNotRunning,
    #[error("No results found")]
    NoResults,
    #[error("Node.js not found: {0}")]
    NodeNotFound(String),
    #[error("Setup error: {0}")]
    Setup(String),
    #[error("Translation error: {0}")]
    Translation(String),
}

/// Translate a URL into paper metadata via the translation server.
pub async fn translate_url(
    server: &TranslationServer,
    url: &str,
) -> Result<Vec<Paper>, TranslateError> {
    let items = server.translate_web(url).await?;
    Ok(items.into_iter().filter_map(|i| i.into_paper()).collect())
}

/// Look up metadata by identifier (DOI, ISBN, PMID, arXiv ID).
pub async fn translate_search(
    server: &TranslationServer,
    identifier: &str,
) -> Result<Vec<Paper>, TranslateError> {
    let items = server.search(identifier).await?;
    Ok(items.into_iter().filter_map(|i| i.into_paper()).collect())
}

/// Import bibliography text (BibTeX, RIS, etc.) into papers.
pub async fn import_bibliography(
    server: &TranslationServer,
    text: &str,
) -> Result<Vec<Paper>, TranslateError> {
    let items = server.import(text).await?;
    Ok(items.into_iter().filter_map(|i| i.into_paper()).collect())
}

/// Export papers to a bibliography format.
pub async fn export_bibliography(
    server: &TranslationServer,
    items: &[ZoteroItem],
    format: &str,
) -> Result<String, TranslateError> {
    server.export(items, format).await
}

//! Integration with the Zotero translation-server for metadata lookup, web
//! scraping, and bibliography import/export via a managed Node.js subprocess.

mod server;
mod translator;

pub use server::TranslationServer;
pub use translator::ZoteroItem;

use rotero_models::Paper;

/// Errors that can occur during translation server operations.
#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    /// An HTTP request to the translation server failed.
    #[error("HTTP error: {0}")]
    Http(String),
    /// The translation server process is not running.
    #[error("Server not running")]
    ServerNotRunning,
    /// The translation returned no matching items.
    #[error("No results found")]
    NoResults,
    /// Failed to install or start the translation server.
    #[error("Setup error: {0}")]
    Setup(String),
    /// The translation server returned an error or unparseable response.
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

//! Integration with the Zotero translation-server for metadata lookup, web
//! scraping, and bibliography import/export via a managed Node.js subprocess.

pub mod html_meta;
mod item;
mod server;
pub mod translators;

pub use html_meta::extract_from_html;
pub use item::ZoteroItem;
pub use server::TranslationServer;
pub use translators::{TranslationContext, Translator, TranslatorRegistry};

use rotero_models::Paper;

/// Errors that can occur during translation and metadata extraction.
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
    /// This translator does not apply to the given input; the registry should
    /// skip it and try the next candidate.
    #[error("Translator not applicable")]
    NotApplicable,
    /// A network request made by a native translator failed.
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    /// A response could not be parsed.
    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),
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

//! In-process metadata extraction: an embedded JS engine that runs upstream
//! Zotero translators unmodified, plus hand-written Rust hub translators
//! (Embedded Metadata, DOI content negotiation) and bibliography import.

#[cfg(feature = "translator-engine")]
pub mod dom;
#[cfg(feature = "translator-engine")]
pub mod engine;
pub mod html_meta;
mod item;
pub mod translators;
pub mod zu;

pub use html_meta::{extract_from_html, extract_zotero_item};
pub use item::ZoteroItem;
pub use translators::{TranslationContext, Translator, TranslatorRegistry};

/// Errors that can occur during translation and metadata extraction.
#[derive(Debug, thiserror::Error)]
pub enum TranslateError {
    /// An HTTP request failed.
    #[error("HTTP error: {0}")]
    Http(String),
    /// A translator returned an error or unparseable response.
    #[error("Translation error: {0}")]
    Translation(String),
    /// This translator does not apply to the given input; the registry should
    /// skip it and try the next candidate.
    #[error("Translator not applicable")]
    NotApplicable,
    /// A network request made by an in-process translator failed.
    #[error("Request error: {0}")]
    Request(#[from] reqwest::Error),
    /// A response could not be parsed.
    #[error("Parse error: {0}")]
    Parse(#[from] serde_json::Error),
}

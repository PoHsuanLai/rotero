//! In-process Rust translators for metadata extraction, an alternative to the
//! Node translation-server subprocess.
//!
//! A [`Translator`] mirrors a Zotero translator's detect-then-extract shape,
//! collapsed into one fallible call. The [`TranslatorRegistry`] fetches a page
//! once and dispatches to the highest-priority translator that applies.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{TranslateError, ZoteroItem};

mod doi_content_negotiation;
mod embedded_metadata;
mod import_formats;
#[cfg(feature = "translator-engine")]
mod js_translator;
mod registry;

pub use doi_content_negotiation::DoiContentNegotiation;
pub use embedded_metadata::EmbeddedMetadata;
pub use import_formats::{ImportFormat, parse_import};
#[cfg(feature = "translator-engine")]
pub use js_translator::JsTranslator;
pub use registry::{TranslatorRegistry, has_usable};

/// The document a translator operates on, plus its provenance.
#[derive(Debug, Clone)]
pub struct TranslationContext {
    /// Final URL after redirects.
    pub url: String,
    /// Response `Content-Type`, if known.
    pub content_type: Option<String>,
    /// The page body (HTML) or raw text (RIS/BibTeX/CSL). Held behind an [`Arc`]
    /// so candidate translators can share it without copying.
    pub body: Arc<str>,
}

/// An in-process translator that extracts [`ZoteroItem`]s from a document.
#[async_trait]
pub trait Translator: Send + Sync {
    /// Stable id (mirrors the Zotero translator label). Used for logging.
    fn id(&self) -> &'static str;

    /// Higher runs first. Generic hubs (Embedded Metadata) sit low so that
    /// site-specific translators win when both apply.
    fn priority(&self) -> i32 {
        100
    }

    /// Cheap URL prefilter. Default matches everything; site translators
    /// override to match their domain. Hides the match strategy (regex,
    /// content-type, all) from the registry.
    fn matches_url(&self, _url: &str) -> bool {
        true
    }

    /// Cheap applicability check on the fetched context. Default `true` (let
    /// [`translate`](Translator::translate) decide). Translators for which
    /// running the full extraction is expensive override this.
    fn detect(&self, _ctx: &TranslationContext) -> bool {
        true
    }

    /// Extract items. Return [`TranslateError::NotApplicable`] to signal the
    /// registry to skip this translator and try the next candidate.
    async fn translate(
        &self,
        ctx: &TranslationContext,
    ) -> Result<Vec<ZoteroItem>, TranslateError>;
}

/// Fetch a URL and build a [`TranslationContext`]. Shared by the registry and
/// the connector's scrape path: validates the scheme (SSRF guard), follows up
/// to 10 redirects, and uses a browser-like User-Agent.
pub async fn fetch_context(url: &str) -> Result<TranslationContext, TranslateError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|e| TranslateError::Http(format!("Invalid URL: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(TranslateError::Http(format!(
                "Unsupported URL scheme: {scheme}"
            )));
        }
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let resp = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; Rotero/0.1; +https://github.com/rotero)",
        )
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(TranslateError::Http(format!("HTTP {} for {url}", resp.status())));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let final_url = resp.url().to_string();
    let body = resp.text().await?;

    Ok(TranslationContext {
        url: final_url,
        content_type,
        body: Arc::from(body),
    })
}

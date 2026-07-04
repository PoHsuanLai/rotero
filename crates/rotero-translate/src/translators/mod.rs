//! In-process Rust translators: the extensibility spine that lets metadata
//! extraction run without the Node translation-server subprocess.
//!
//! A [`Translator`] mirrors a Zotero translator's shape (detect + do), but
//! collapsed into one fallible call. The [`TranslatorRegistry`] fetches a page
//! once and dispatches to the highest-priority translator that applies. Phase-2
//! XPath/JS translators implement the *same* trait and slot into the registry
//! with no dispatch changes.

use std::sync::Arc;

use async_trait::async_trait;

use crate::{TranslateError, ZoteroItem};

mod registry;

pub use registry::{TranslatorRegistry, has_usable};

/// The document a translator operates on, plus its provenance.
///
/// Phase 1 carries the fetched body + final URL. Phase 2 adds a parsed-DOM
/// handle here (for the XPath engine) without changing the [`Translator`] trait.
#[derive(Debug, Clone)]
pub struct TranslationContext {
    /// Final URL after redirects.
    pub url: String,
    /// Response `Content-Type`, if known.
    pub content_type: Option<String>,
    /// The page body (HTML) or raw text (RIS/BibTeX/CSL). `Arc` so many
    /// candidate translators can inspect the same page without copying it.
    pub body: Arc<str>,
    // phase 2: pub dom: Option<ParsedDom>,
}

/// A native (in-process) translator. One impl per Zotero "hub" translator in
/// phase 1; phase-2 site translators implement the same trait.
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

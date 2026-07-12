//! In-process translators for metadata extraction — the corpus JS engine plus
//! hand-written Rust hub translators, dispatched by a shared registry.
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
#[cfg(feature = "translator-engine")]
mod loader;
mod registry;

pub use doi_content_negotiation::DoiContentNegotiation;
pub use embedded_metadata::EmbeddedMetadata;
pub use import_formats::{ImportFormat, parse_import};
#[cfg(feature = "translator-engine")]
pub use js_translator::JsTranslator;
pub use registry::{TranslatorRegistry, has_usable};

/// The document a translator operates on, plus its provenance.
#[derive(Clone)]
pub struct TranslationContext {
    /// Final URL after redirects.
    pub url: String,
    /// Response `Content-Type`, if known.
    pub content_type: Option<String>,
    /// The page body (HTML) or raw text (RIS/BibTeX/CSL). Held behind an [`Arc`]
    /// so candidate translators can share it without copying. For the extension's
    /// send-HTML path this is the *rendered* DOM (`outerHTML`).
    pub body: Arc<str>,
    /// Optional *raw server HTML* (a page-context fetch of the page's own URL),
    /// carrying inline `<script>` data an SPA strips from the rendered [`body`].
    /// When present, JS translators parse against this instead. `None` for the
    /// server-fetch path and offline tests, preserving prior behavior.
    pub raw_body: Option<Arc<str>>,
    /// Optional queue that routes a JS translator's follow-up HTTP requests to an
    /// external driver (the connector's browser-proxy loop), so they run in the
    /// user's authenticated tab. `None` uses the anonymous direct fetch.
    #[cfg(feature = "translator-engine")]
    pub fetch_queue: Option<tokio::sync::mpsc::UnboundedSender<crate::engine::BrokeredFetch>>,
}

impl std::fmt::Debug for TranslationContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranslationContext")
            .field("url", &self.url)
            .field("content_type", &self.content_type)
            .field("body_len", &self.body.len())
            .field("has_raw_body", &self.raw_body.is_some())
            .finish()
    }
}

impl TranslationContext {
    /// Build a context with no follow-up-fetch broker (the common case: server
    /// fetches, offline tests, and non-brokered translation). The optional
    /// browser-proxy queue is attached separately via [`with_fetch_queue`].
    ///
    /// [`with_fetch_queue`]: TranslationContext::with_fetch_queue
    pub fn new(
        url: String,
        content_type: Option<String>,
        body: Arc<str>,
        raw_body: Option<Arc<str>>,
    ) -> Self {
        Self {
            url,
            content_type,
            body,
            raw_body,
            #[cfg(feature = "translator-engine")]
            fetch_queue: None,
        }
    }

    /// Attach a queue that routes JS translators' follow-up fetches to an external
    /// driver (the connector's browser-proxy loop).
    #[cfg(feature = "translator-engine")]
    pub fn with_fetch_queue(
        mut self,
        queue: tokio::sync::mpsc::UnboundedSender<crate::engine::BrokeredFetch>,
    ) -> Self {
        self.fetch_queue = Some(queue);
        self
    }
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
    async fn translate(&self, ctx: &TranslationContext) -> Result<Vec<ZoteroItem>, TranslateError>;
}

/// Fetch a URL and build a [`TranslationContext`]. Shared by the registry and
/// the connector's scrape path: validates the scheme (SSRF guard), follows up
/// to 10 redirects, and uses a browser-like User-Agent.
pub async fn fetch_context(url: &str) -> Result<TranslationContext, TranslateError> {
    let parsed =
        reqwest::Url::parse(url).map_err(|e| TranslateError::Http(format!("Invalid URL: {e}")))?;
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
        return Err(TranslateError::Http(format!(
            "HTTP {} for {url}",
            resp.status()
        )));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let final_url = resp.url().to_string();
    let body = resp.text().await?;

    Ok(TranslationContext::new(
        final_url,
        content_type,
        Arc::from(body),
        None,
    ))
}

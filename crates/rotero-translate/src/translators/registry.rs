//! The translator registry: fetch a page once, dispatch to the highest-priority
//! [`Translator`] that applies.

use crate::{TranslateError, ZoteroItem};

use super::{
    DoiContentNegotiation, EmbeddedMetadata, ImportFormat, TranslationContext, Translator,
    fetch_context, parse_import,
};

/// Holds the set of in-process translators and dispatches web/import requests.
pub struct TranslatorRegistry {
    translators: Vec<Box<dyn Translator>>,
}

impl TranslatorRegistry {
    /// Construct a registry with the built-in translators registered. Any URL
    /// not handled by a translator here returns `None` from
    /// [`translate_web`](Self::translate_web), so callers fall through to the
    /// Node/scrape tiers.
    pub fn with_builtins() -> Self {
        let mut translators: Vec<Box<dyn Translator>> = vec![
            Box::new(DoiContentNegotiation),
            Box::new(EmbeddedMetadata),
        ];
        register_js_translators(&mut translators);
        Self { translators }
    }

    /// Translate a web URL. Fetches the page once, then dispatches via
    /// [`translate_context`](Self::translate_context). Returns `None` if no
    /// translator produced a usable item.
    pub async fn translate_web(&self, url: &str) -> Option<Vec<ZoteroItem>> {
        // Cheap URL prefilter before paying for a fetch.
        if !self.translators.iter().any(|t| t.matches_url(url)) {
            return None;
        }

        let ctx = match fetch_context(url).await {
            Ok(ctx) => ctx,
            Err(e) => {
                tracing::debug!("translator fetch failed for {url}: {e}");
                return None;
            }
        };
        self.translate_context(&ctx).await
    }

    /// Translate a page whose HTML the caller already has (e.g. the browser
    /// extension captured the rendered, authenticated DOM), skipping the network
    /// fetch. This sidesteps publisher anti-bot walls that block a server-side
    /// re-fetch: the extension sees the real page, so the connector does too.
    pub async fn translate_html(&self, url: &str, html: &str) -> Option<Vec<ZoteroItem>> {
        if !self.translators.iter().any(|t| t.matches_url(url)) {
            return None;
        }
        let ctx = TranslationContext {
            url: url.to_string(),
            content_type: Some("text/html".to_string()),
            body: std::sync::Arc::from(html),
        };
        self.translate_context(&ctx).await
    }

    /// Dispatch an already-fetched page: run the highest-priority applicable
    /// translator, falling through on `NotApplicable`, error, or an unusable
    /// result. Returns `None` if none produced a usable item.
    ///
    /// Separated from [`translate_web`](Self::translate_web) so callers with a
    /// page already in hand (offline tests, the differential harness) can
    /// dispatch without a network fetch.
    pub async fn translate_context(&self, ctx: &TranslationContext) -> Option<Vec<ZoteroItem>> {
        let mut candidates: Vec<&dyn Translator> = self
            .translators
            .iter()
            .map(Box::as_ref)
            .filter(|t| t.matches_url(&ctx.url) && t.detect(ctx))
            .collect();
        candidates.sort_by_key(|t| std::cmp::Reverse(t.priority()));

        for t in candidates {
            match t.translate(ctx).await {
                Ok(mut items) if has_usable(&items) => {
                    enrich_from_embedded_metadata(&mut items, ctx);
                    return Some(items);
                }
                Ok(_) => continue,
                Err(e) => {
                    tracing::debug!("translator {} failed for {}: {e}", t.id(), ctx.url);
                }
            }
        }
        None
    }

    /// Parse pasted/loaded bibliography text (RIS, BibTeX, CSL-JSON, NBIB) into
    /// items, sniffing the format from the content. The returned items carry
    /// local PDF paths (from BibTeX `file` fields) as attachments.
    pub fn translate_import(&self, text: &str) -> Result<Vec<ZoteroItem>, TranslateError> {
        parse_import(text, ImportFormat::sniff(text))
    }
}

impl Default for TranslatorRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

/// Fill fields a site translator left empty from the page's embedded metadata
/// (Highwire `citation_*` / Dublin Core / body abstract). Site translators are
/// authoritative — this only *adds* to empty fields, never overwrites — so it's
/// a safe, general backstop for the fields a translator's own parse dropped
/// (e.g. our engine's XPath missing `fpage`/`abstract` on PMC's efetch XML,
/// where the page still carries `citation_firstpage` and a body abstract).
///
/// No-ops for non-HTML contexts (import/RIS/BibTeX) and when the page has no
/// usable embedded metadata.
fn enrich_from_embedded_metadata(items: &mut [ZoteroItem], ctx: &TranslationContext) {
    // Only HTML pages carry the meta tags / body abstract this reads.
    let is_html = ctx
        .content_type
        .as_deref()
        .map(|ct| ct.contains("html"))
        .unwrap_or(true);
    if !is_html {
        return;
    }

    let em = crate::html_meta::extract_zotero_item(&ctx.body);
    // Nothing to contribute if the page has no scholarly metadata.
    if em.title.is_empty() {
        return;
    }

    let fill = |dst: &mut String, src: &str| {
        if dst.is_empty() && !src.is_empty() {
            *dst = src.to_string();
        }
    };

    for item in items.iter_mut() {
        // Don't enrich notes/attachments — they aren't bibliographic records.
        if item.item_type == "note" || item.item_type == "attachment" {
            continue;
        }
        fill(&mut item.abstract_note, &em.abstract_note);
        fill(&mut item.pages, &em.pages);
        fill(&mut item.volume, &em.volume);
        fill(&mut item.issue, &em.issue);
        fill(&mut item.date, &em.date);
        fill(&mut item.doi, &em.doi);
        fill(&mut item.issn, &em.issn);
        fill(&mut item.isbn, &em.isbn);
        fill(&mut item.publication_title, &em.publication_title);
        fill(&mut item.publisher, &em.publisher);
        fill(&mut item.language, &em.language);
        if item.creators.is_empty() {
            item.creators = em.creators.clone();
        }
        // The DOI-content-negotiation translator (and CrossRef) always stamps
        // "journalArticle"; trust the page when it declares something more
        // specific (conferencePaper, bookSection, …).
        if item.item_type == "journalArticle"
            && !em.item_type.is_empty()
            && em.item_type != "journalArticle"
        {
            item.item_type = em.item_type.clone();
        }
    }
}

/// Whether an item list contains at least one usable item: a non-note,
/// non-attachment item with a non-empty title. Applied identically across the
/// in-process, Node, and scrape tiers.
pub fn has_usable(items: &[ZoteroItem]) -> bool {
    items
        .iter()
        .any(|i| i.item_type != "note" && i.item_type != "attachment" && !i.title.is_empty())
}

/// Load the upstream `zotero/translators` corpus (feature-gated) from disk and
/// register each as a [`JsTranslator`]. A missing corpus directory yields no JS
/// translators — the registry then behaves like the pre-engine build.
#[cfg(feature = "translator-engine")]
fn register_js_translators(translators: &mut Vec<Box<dyn Translator>>) {
    let Some(dir) = super::loader::corpus_dir() else {
        tracing::info!("no translator corpus found; using built-in hubs only");
        return;
    };
    for t in super::loader::load_from_dir(&dir) {
        translators.push(Box::new(t));
    }
}

/// No-op when the engine feature is off: the registry ships only the built-in
/// hubs, identical to the pre-engine build.
#[cfg(not(feature = "translator-engine"))]
fn register_js_translators(_translators: &mut Vec<Box<dyn Translator>>) {}

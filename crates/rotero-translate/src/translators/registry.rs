//! The translator registry: fetch a page once, dispatch to the highest-priority
//! [`Translator`] that applies.

use crate::ZoteroItem;

use super::{DoiContentNegotiation, EmbeddedMetadata, Translator, fetch_context};

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
        Self {
            translators: vec![
                Box::new(DoiContentNegotiation),
                Box::new(EmbeddedMetadata),
            ],
        }
    }

    /// Translate a web URL. Fetches the page once, then runs the
    /// highest-priority applicable translator, falling through on
    /// `NotApplicable`, error, or an unusable result. Returns `None` if no
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

        let mut candidates: Vec<&dyn Translator> = self
            .translators
            .iter()
            .map(Box::as_ref)
            .filter(|t| t.matches_url(&ctx.url) && t.detect(&ctx))
            .collect();
        candidates.sort_by_key(|t| std::cmp::Reverse(t.priority()));

        for t in candidates {
            match t.translate(&ctx).await {
                Ok(items) if has_usable(&items) => return Some(items),
                Ok(_) => continue,
                Err(e) => {
                    tracing::debug!("translator {} failed for {}: {e}", t.id(), ctx.url);
                }
            }
        }
        None
    }
}

impl Default for TranslatorRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

/// Whether an item list contains at least one usable item — a non-note,
/// non-attachment item with a non-empty title. This is the shared acceptance
/// test applied identically across the native, Node, and scrape tiers.
pub fn has_usable(items: &[ZoteroItem]) -> bool {
    items
        .iter()
        .any(|i| i.item_type != "note" && i.item_type != "attachment" && !i.title.is_empty())
}

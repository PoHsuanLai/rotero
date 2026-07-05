//! Extracts scholarly metadata from a page's `<meta>` tags (Highwire
//! `citation_*`, Dublin Core, PRISM, bepress, eprints, OpenGraph) and JSON-LD,
//! mirroring Zotero's "Embedded Metadata" translator.
//!
//! Runs at low priority so site-specific translators take precedence when both
//! apply.

use async_trait::async_trait;

use crate::html_meta::extract_zotero_item;
use crate::item::ZoteroItem;

use super::{TranslationContext, Translator};

/// Generic scholarly-metadata extractor over `<meta>` tags + JSON-LD.
pub struct EmbeddedMetadata;

#[async_trait]
impl Translator for EmbeddedMetadata {
    fn id(&self) -> &'static str {
        "Embedded Metadata"
    }

    /// Low: this is the generic fallback; bespoke site translators outrank it.
    fn priority(&self) -> i32 {
        50
    }

    async fn translate(
        &self,
        ctx: &TranslationContext,
    ) -> Result<Vec<ZoteroItem>, crate::TranslateError> {
        let item = extract_zotero_item(&ctx.body);
        // Stamp the fetched URL if the page didn't carry one.
        let item = ZoteroItem {
            url: if item.url.is_empty() {
                ctx.url.clone()
            } else {
                item.url
            },
            ..item
        };
        if item.title.is_empty() {
            return Err(crate::TranslateError::NotApplicable);
        }
        Ok(vec![item])
    }
}

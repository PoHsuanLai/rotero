//! End-to-end tests for vendored upstream translators run through the JS engine
//! (2a). Each asserts that an unmodified `zotero/translators/*.js` file, driven
//! by [`JsTranslator`], extracts the expected fields from a real saved page.
//! Gated on the `translator-engine` feature.
#![cfg(feature = "translator-engine")]

use std::sync::Arc;

use rotero_translate::translators::{JsTranslator, TranslationContext, Translator};

/// The vendored translator source, embedded the same way the registry loads it.
const THEORY_OF_COMPUTING: &str = include_str!("../vendor/translators/Theory of Computing.js");

const V009A013_HTML: &str = include_str!("fixtures/theory_of_computing_v009a013.html");

#[tokio::test]
async fn theory_of_computing_extracts_article() {
    let t = JsTranslator::from_source(THEORY_OF_COMPUTING).expect("parse vendored translator");

    let url = "http://toc.nada.kth.se/articles/v009a013/index.html";
    assert!(t.matches_url(url), "target regex should match the article URL");

    let ctx = TranslationContext {
        url: url.to_string(),
        content_type: Some("text/html".to_string()),
        body: Arc::from(V009A013_HTML),
    };

    let items = t.translate(&ctx).await.expect("translate");
    assert_eq!(items.len(), 1, "one article expected");
    let item = &items[0];

    // Field-by-field against the translator's own bundled test expectations.
    assert_eq!(item.item_type, "journalArticle");
    assert_eq!(item.title, "Optimal Hitting Sets for Combinatorial Shapes");
    assert_eq!(item.doi, "10.4086/toc.2013.v009a013");
    assert_eq!(item.volume, "9");
    assert_eq!(item.pages, "441–470");

    let last_names: Vec<&str> = item.creators.iter().map(|c| c.last_name.as_str()).collect();
    assert_eq!(last_names, ["Bhaskara", "Desai", "Srinivasan"]);
}

#[tokio::test]
async fn theory_of_computing_ignores_unrelated_url() {
    let t = JsTranslator::from_source(THEORY_OF_COMPUTING).expect("parse");
    assert!(!t.matches_url("https://arxiv.org/abs/1234.5678"));
}

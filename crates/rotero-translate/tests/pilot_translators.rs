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

const NATURE_PUBLISHING_GROUP: &str =
    include_str!("../vendor/translators/Nature Publishing Group.js");

/// A trimmed real Nature article page (meta tags only, no citation-export link
/// so the RIS fetch is skipped and the test stays offline).
const NATURE_HTML: &str = include_str!("fixtures/nature_nature12373.html");

const FRONTIERS: &str = include_str!("../vendor/translators/Frontiers.js");

/// A real Frontiers article page (meta tags include two `citation_author`s and a
/// `citation_abstract`).
const FRONTIERS_HTML: &str = include_str!("fixtures/frontiers_fpsyg_2011_00326.html");

/// A real PubMed Central article page. Its `citation_firstpage` has no matching
/// `citation_lastpage`, and the abstract is rendered in the page body (the only
/// abstract meta is a truncated `description` snippet).
const PMC_HTML: &str = include_str!("fixtures/pmc_PMC2377243.html");

#[tokio::test]
async fn theory_of_computing_extracts_article() {
    let t = JsTranslator::from_source(THEORY_OF_COMPUTING).expect("parse vendored translator");

    let url = "http://toc.nada.kth.se/articles/v009a013/index.html";
    assert!(
        t.matches_url(url),
        "target regex should match the article URL"
    );

    let ctx = TranslationContext::new(
        url.to_string(),
        Some("text/html".to_string()),
        Arc::from(V009A013_HTML),
        None,
    );

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

/// The Nature translator delegates extraction to the Embedded Metadata hub via
/// `Zotero.loadTranslator("web")` + `getTranslatorObject().doWeb()`, enriches the
/// item, and (offline) skips its RIS-supplement fetch. Regression guard: this
/// exercises the `String.prototype.substr` polyfill (hit by ZU.cleanISSN), the
/// element `.href` accessor, and the loadTranslator delegation completing an item
/// even when the RIS scraper produces nothing.
#[tokio::test]
async fn nature_extracts_article_via_embedded_metadata() {
    let t = JsTranslator::from_source(NATURE_PUBLISHING_GROUP)
        .expect("parse vendored Nature translator");

    let url = "https://www.nature.com/articles/nature12373";
    assert!(t.matches_url(url), "Nature target regex should match");

    let ctx = TranslationContext::new(
        url.to_string(),
        Some("text/html".to_string()),
        Arc::from(NATURE_HTML),
        None,
    );

    let items = t.translate(&ctx).await.expect("translate");
    assert_eq!(items.len(), 1, "one journal article expected");
    let item = &items[0];
    assert_eq!(item.item_type, "journalArticle");
    assert_eq!(item.title, "Nanometre-scale thermometry in a living cell");
    assert!(!item.creators.is_empty(), "expected at least one author");
    assert!(
        !item.abstract_note.is_empty(),
        "expected a non-empty abstract"
    );
}

#[tokio::test]
async fn theory_of_computing_ignores_unrelated_url() {
    let t = JsTranslator::from_source(THEORY_OF_COMPUTING).expect("parse");
    assert!(!t.matches_url("https://arxiv.org/abs/1234.5678"));
}

/// The full registry (built-in hubs + the loaded corpus) dispatches a Theory of
/// Computing page to its translator and extracts the article — proving the
/// corpus load, URL dispatch, and JS run compose end-to-end via with_builtins()
/// and translate_context (offline, no network).
#[tokio::test]
async fn registry_dispatches_corpus_translator() {
    use rotero_translate::translators::TranslatorRegistry;

    let registry = TranslatorRegistry::with_builtins();
    let ctx = TranslationContext::new(
        "http://theoryofcomputing.org/articles/v009a013/".to_string(),
        Some("text/html".to_string()),
        Arc::from(V009A013_HTML),
        None,
    );

    let items = registry
        .translate_context(&ctx)
        .await
        .expect("corpus should dispatch the Theory of Computing translator");
    assert!(
        items
            .iter()
            .any(|i| i.title == "Optimal Hitting Sets for Combinatorial Shapes"),
        "expected the ToC translator's article, got: {:?}",
        items.iter().map(|i| &i.title).collect::<Vec<_>>()
    );
}

/// translate_html dispatches caller-supplied HTML without any network fetch —
/// the path the browser extension uses to hand the connector the real,
/// authenticated page and sidestep publisher anti-bot walls.
#[tokio::test]
async fn translate_html_dispatches_without_fetch() {
    use rotero_translate::translators::TranslatorRegistry;

    let registry = TranslatorRegistry::with_builtins();
    // A URL whose server-side fetch would be irrelevant here — only the passed
    // HTML is used. Use the ToC fixture + a matching URL.
    let items = registry
        .translate_html(
            "http://theoryofcomputing.org/articles/v009a013/",
            V009A013_HTML,
        )
        .await
        .expect("translate_html should dispatch from the supplied HTML");
    assert!(
        items
            .iter()
            .any(|i| i.title == "Optimal Hitting Sets for Combinatorial Shapes"),
        "translate_html should extract from the given HTML"
    );
}

/// The Frontiers translator delegates extraction to the Embedded Metadata hub
/// via the *promise* form `let em = await translator.getTranslatorObject();
/// await em.doWeb(...)` — unlike Nature, which uses the callback form. Before the
/// shim returned the delegate proxy from `getTranslatorObject()`, that `await`
/// yielded `undefined`, `em.doWeb` threw, the delegation dropped every field, the
/// translator returned nothing, and the registry fell through to a CrossRef
/// lookup that reports only one author for this DOI. Regression guard: the two
/// `citation_author`s and the `citation_abstract` must survive the delegation.
#[tokio::test]
async fn frontiers_extracts_both_authors_and_abstract() {
    let t = JsTranslator::from_source(FRONTIERS).expect("parse vendored Frontiers translator");

    let url =
        "https://www.frontiersin.org/journals/psychology/articles/10.3389/fpsyg.2011.00326/full";
    assert!(t.matches_url(url), "Frontiers target regex should match");

    let ctx = TranslationContext::new(
        url.to_string(),
        Some("text/html".to_string()),
        Arc::from(FRONTIERS_HTML),
        None,
    );

    let items = t.translate(&ctx).await.expect("translate");
    assert_eq!(items.len(), 1, "one journal article expected");
    let item = &items[0];

    let last_names: Vec<&str> = item.creators.iter().map(|c| c.last_name.as_str()).collect();
    assert_eq!(
        last_names,
        ["Crouzet", "Serre"],
        "both citation_authors must survive the Embedded Metadata delegation"
    );
    assert!(
        !item.abstract_note.is_empty(),
        "citation_abstract must survive the delegation"
    );
}

/// The PMC page dispatches to the vendored `PubMed Central.js` (priority 100),
/// which mines its bibliographic data from an NLM efetch XML fetch. Two fields —
/// `pages` (from `fpage`) and the abstract — don't survive that path in the
/// engine's XPath, but the page itself carries `citation_firstpage` and a body
/// abstract. The registry's embedded-metadata enrichment backfills exactly those
/// empty fields, so the final item has pages and a full abstract.
///
/// Runs offline: with no network the efetch fails and the registry falls through
/// to the Embedded Metadata hub, which reads the same page fields — so the item
/// still carries pages and abstract either way.
#[tokio::test]
async fn pmc_registry_backfills_pages_and_abstract() {
    use rotero_translate::translators::TranslatorRegistry;

    let registry = TranslatorRegistry::with_builtins();
    let items = registry
        .translate_html(
            "https://www.ncbi.nlm.nih.gov/pmc/articles/PMC2377243/",
            PMC_HTML,
        )
        .await
        .expect("PMC page should produce an item");
    let item = &items[0];

    assert_eq!(item.item_type, "journalArticle");
    assert!(
        item.pages.contains("37"),
        "pages should be backfilled from citation_firstpage, got {:?}",
        item.pages
    );
    assert!(
        item.abstract_note.contains("long-term oxygen therapy")
            && item.abstract_note.contains("guinea pig"),
        "abstract should be the full body text"
    );
    assert!(
        item.abstract_note.len() > 1000,
        "expected the full body abstract (not the truncated meta snippet), got {} chars",
        item.abstract_note.len()
    );
    assert_eq!(item.creators.len(), 7, "seven authors expected");
}

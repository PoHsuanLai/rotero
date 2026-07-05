//! End-to-end tests for the JS translator engine (phase 2). Runs
//! translator-shaped JavaScript through `boa` + the sandbox against fixture
//! HTML and asserts the emitted `ZoteroItem`. Gated on the `translator-engine`
//! feature.
#![cfg(feature = "translator-engine")]

use rotero_translate::engine::run_web_translator;

/// A translator that reads Highwire `citation_*` meta tags via ZU.xpathText and
/// emits a journalArticle — the shape a huge fraction of real translators take.
const META_TRANSLATOR: &str = r#"
function detectWeb(doc, url) {
    return ZU.xpathText(doc, '//meta[@name="citation_title"]/@content') ? "journalArticle" : false;
}
function doWeb(doc, url) {
    var item = new Zotero.Item("journalArticle");
    item.title = ZU.xpathText(doc, '//meta[@name="citation_title"]/@content');
    item.DOI = ZU.cleanDOI(ZU.xpathText(doc, '//meta[@name="citation_doi"]/@content'));
    var authors = ZU.xpath(doc, '//meta[@name="citation_author"]/@content');
    for (var i = 0; i < authors.length; i++) {
        item.creators.push(ZU.cleanAuthor(authors[i].textContent, "author", true));
    }
    item.complete();
}
"#;

const META_HTML: &str = r#"<html><head>
    <meta name="citation_title" content="Attention Is All You Need">
    <meta name="citation_doi" content="10.5555/3295222.3295349">
    <meta name="citation_author" content="Vaswani, Ashish">
    <meta name="citation_author" content="Shazeer, Noam">
</head><body></body></html>"#;

#[test]
fn runs_meta_translator_end_to_end() {
    let items = run_web_translator(META_TRANSLATOR, META_HTML, "https://example.org/paper")
        .expect("engine run");
    assert_eq!(items.len(), 1, "expected one emitted item");
    let item = &items[0];
    assert_eq!(item.item_type, "journalArticle");
    assert_eq!(item.title, "Attention Is All You Need");
    assert_eq!(item.doi, "10.5555/3295222.3295349");
    assert_eq!(item.creators.len(), 2);
    assert_eq!(item.creators[0].last_name, "Vaswani");
    assert_eq!(item.creators[0].first_name, "Ashish");
    assert_eq!(item.creators[0].creator_type, "author");
}

#[test]
fn detect_web_gates_do_web() {
    // Page without citation_title → detectWeb returns false → no items.
    let html = r#"<html><head><title>Not a paper</title></head><body></body></html>"#;
    let items = run_web_translator(META_TRANSLATOR, html, "https://example.org/x").expect("run");
    assert!(items.is_empty(), "detectWeb should have gated doWeb");
}

/// A translator that reads a table (exercises the <tbody> rewrite path end to
/// end: the JS uses //table/tr, the DOM adapter rewrites it so it matches).
const TABLE_TRANSLATOR: &str = r#"
function detectWeb(doc, url) { return "journalArticle"; }
function doWeb(doc, url) {
    var item = new Zotero.Item("journalArticle");
    item.title = ZU.xpathText(doc, '//table/tr/td[1]');
    item.date = ZU.xpathText(doc, '//table/tr/td[2]');
    item.complete();
}
"#;

#[test]
fn table_xpath_matches_through_tbody_rewrite() {
    // Source HTML has no <tbody>; the adapter's rewrite lets //table/tr match.
    let html = r#"<html><body><table><tr><td>Paper Title</td><td>2020</td></tr></table></body></html>"#;
    let items = run_web_translator(TABLE_TRANSLATOR, html, "https://example.org/t").expect("run");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Paper Title");
    assert_eq!(items[0].date, "2020");
}

/// A translator that reads the page via the CSS DOM API — `text()`, `attr()`,
/// and scoped `querySelectorAll` — the shape 57% of upstream translators use.
const CSS_TRANSLATOR: &str = r#"
function detectWeb(doc, url) {
    return doc.querySelector("h1.article-title") ? "journalArticle" : false;
}
function doWeb(doc, url) {
    var item = new Zotero.Item("journalArticle");
    item.title = text(doc, "h1.article-title");
    item.DOI = attr(doc, "a.doi", "data-doi");
    var rows = doc.querySelectorAll("ul.authors > li");
    for (var i = 0; i < rows.length; i++) {
        item.creators.push(ZU.cleanAuthor(text(rows[i], "span.name"), "author", false));
    }
    item.complete();
}
"#;

const CSS_HTML: &str = r#"<html><head></head><body>
    <h1 class="article-title">  On   CSS   Selectors </h1>
    <a class="doi" data-doi="10.1000/css.42">link</a>
    <ul class="authors">
        <li><span class="name">Ada Lovelace</span></li>
        <li><span class="name">Alan Turing</span></li>
    </ul>
</body></html>"#;

#[test]
fn runs_css_translator_end_to_end() {
    let items = run_web_translator(CSS_TRANSLATOR, CSS_HTML, "https://example.org/paper")
        .expect("engine run");
    assert_eq!(items.len(), 1, "expected one emitted item");
    let item = &items[0];
    assert_eq!(item.title, "On CSS Selectors", "text() should collapse whitespace");
    assert_eq!(item.doi, "10.1000/css.42");
    assert_eq!(item.creators.len(), 2, "scoped querySelectorAll + text()");
    assert_eq!(item.creators[0].first_name, "Ada");
    assert_eq!(item.creators[0].last_name, "Lovelace");
    assert_eq!(item.creators[1].last_name, "Turing");
}

/// A translator that gates on `doc.location.pathname` (as arXiv's search path
/// detection does) — verifies the driver populates location from the URL.
const LOCATION_TRANSLATOR: &str = r#"
function detectWeb(doc, url) {
    return doc.location.pathname.startsWith("/abs/") ? "preprint" : false;
}
function doWeb(doc, url) {
    var item = new Zotero.Item("preprint");
    item.title = "loc:" + doc.location.pathname;
    item.complete();
}
"#;

#[test]
fn doc_location_is_populated_from_url() {
    let html = r#"<html><body></body></html>"#;
    let items = run_web_translator(LOCATION_TRANSLATOR, html, "https://arxiv.org/abs/1234.5678?x=1")
        .expect("run");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "loc:/abs/1234.5678");

    // A non-/abs/ path should gate doWeb off.
    let none = run_web_translator(LOCATION_TRANSLATOR, html, "https://arxiv.org/list/cs").expect("run");
    assert!(none.is_empty());
}

/// A site translator that delegates extraction to Embedded Metadata via
/// Zotero.loadTranslator("web"), enriching the item in the itemDone handler —
/// the dominant delegation pattern (~58% of upstream translators load another).
const DELEGATING_TRANSLATOR: &str = r#"
function detectWeb(doc, url) { return "journalArticle"; }
function doWeb(doc, url) {
    var translator = Zotero.loadTranslator("web");
    translator.setTranslator("951c027d-74ac-47d4-a107-9c3069ab7b48"); // Embedded Metadata
    translator.setDocument(doc);
    translator.setHandler("itemDone", function (obj, item) {
        // Enrich with something the meta tags didn't carry, then complete.
        item.extra = "delegated";
        item.complete();
    });
    translator.translate();
}
"#;

const EMBEDDED_HTML: &str = r#"<html><head>
    <meta name="citation_title" content="Delegated Extraction Works">
    <meta name="citation_doi" content="10.1234/deleg.1">
    <meta name="citation_author" content="Turing, Alan">
</head><body></body></html>"#;

#[test]
fn load_translator_delegates_to_embedded_metadata() {
    let items = run_web_translator(DELEGATING_TRANSLATOR, EMBEDDED_HTML, "https://pub.example.org/a")
        .expect("run");
    assert_eq!(items.len(), 1, "delegation should yield the hub's item");
    let item = &items[0];
    // Fields came from Embedded Metadata (the delegate)...
    assert_eq!(item.title, "Delegated Extraction Works");
    assert_eq!(item.doi, "10.1234/deleg.1");
    assert_eq!(item.creators.len(), 1);
    assert_eq!(item.creators[0].last_name, "Turing");
    // ...and the itemDone handler's enrichment survived.
    assert_eq!(item.extra, "delegated");
}

#[test]
fn load_translator_unknown_uuid_yields_nothing() {
    // A delegate we don't bridge (e.g. a search/import translator) → no items,
    // so the outer translator produces nothing rather than crashing.
    let src = r#"
    function detectWeb(doc, url) { return "journalArticle"; }
    function doWeb(doc, url) {
        var t = Zotero.loadTranslator("web");
        t.setTranslator("00000000-0000-0000-0000-000000000000");
        t.setDocument(doc);
        t.setHandler("itemDone", function (obj, item) { item.complete(); });
        t.translate();
    }
    "#;
    let items = run_web_translator(src, EMBEDDED_HTML, "https://pub.example.org/a").expect("run");
    assert!(items.is_empty());
}

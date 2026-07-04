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

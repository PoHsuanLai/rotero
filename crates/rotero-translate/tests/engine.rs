//! End-to-end tests for the JS translator engine. Runs translator-shaped
//! JavaScript through `boa` + the sandbox against fixture HTML and asserts the
//! emitted `ZoteroItem`. Gated on the `translator-engine` feature.
#![cfg(feature = "translator-engine")]

use rotero_translate::engine::{
    BrokeredFetch, ChannelBroker, run_web_translator, run_web_translator_raw,
    run_web_translator_with_broker,
};

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

/// A translator that emits a relative PDF attachment URL and a relative item
/// URL. The engine must resolve both against the page URL so downstream
/// consumers (the Find-PDF download) get fetchable absolute URLs.
const RELATIVE_URL_TRANSLATOR: &str = r#"
function detectWeb(doc, url) { return "journalArticle"; }
function doWeb(doc, url) {
    var item = new Zotero.Item("journalArticle");
    item.title = "Relative Links";
    item.url = "../abs/123";
    item.attachments.push({ title: "Full Text PDF", url: "paper.pdf", mimeType: "application/pdf" });
    item.complete();
}
"#;

#[test]
fn relative_attachment_urls_are_resolved_against_the_page() {
    let items = run_web_translator(
        RELATIVE_URL_TRANSLATOR,
        "<html><body></body></html>",
        "https://example.org/journal/pdf/123",
    )
    .expect("run");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(
        item.pdf_url().as_deref(),
        Some("https://example.org/journal/pdf/paper.pdf"),
        "relative PDF url should resolve against the page"
    );
    assert_eq!(item.url, "https://example.org/journal/abs/123");
}

/// Absolute attachment URLs must pass through unchanged.
const ABSOLUTE_URL_TRANSLATOR: &str = r#"
function detectWeb(doc, url) { return "preprint"; }
function doWeb(doc, url) {
    var item = new Zotero.Item("preprint");
    item.title = "Absolute Links";
    item.attachments.push({ title: "Preprint PDF", url: "https://arxiv.org/pdf/2201.00001", mimeType: "application/pdf" });
    item.complete();
}
"#;

#[test]
fn absolute_attachment_urls_pass_through() {
    let items = run_web_translator(
        ABSOLUTE_URL_TRANSLATOR,
        "<html><body></body></html>",
        "https://arxiv.org/abs/2201.00001",
    )
    .expect("run");
    assert_eq!(
        items[0].pdf_url().as_deref(),
        Some("https://arxiv.org/pdf/2201.00001")
    );
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
    let html =
        r#"<html><body><table><tr><td>Paper Title</td><td>2020</td></tr></table></body></html>"#;
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
    assert_eq!(
        item.title, "On CSS Selectors",
        "text() should collapse whitespace"
    );
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
    let items = run_web_translator(
        LOCATION_TRANSLATOR,
        html,
        "https://arxiv.org/abs/1234.5678?x=1",
    )
    .expect("run");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "loc:/abs/1234.5678");

    // A non-/abs/ path should gate doWeb off.
    let none =
        run_web_translator(LOCATION_TRANSLATOR, html, "https://arxiv.org/list/cs").expect("run");
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
    let items = run_web_translator(
        DELEGATING_TRANSLATOR,
        EMBEDDED_HTML,
        "https://pub.example.org/a",
    )
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

/// The other delegation style: the caller gets the delegate's translator object
/// via getTranslatorObject and drives its doWeb itself (as PLoS Journals does).
const GET_TRANSLATOR_OBJECT_STYLE: &str = r#"
function detectWeb(doc, url) { return "journalArticle"; }
function doWeb(doc, url) {
    var t = Zotero.loadTranslator("web");
    t.setTranslator("951c027d-74ac-47d4-a107-9c3069ab7b48"); // Embedded Metadata
    t.setDocument(doc);
    t.setHandler("itemDone", function (obj, item) {
        item.libraryCatalog = "Test Catalog";
        item.complete();
    });
    t.getTranslatorObject(function (trans) { trans.doWeb(doc, url); });
}
"#;

#[test]
fn load_translator_get_translator_object_style() {
    let html = r#"<html><head>
        <meta name="citation_title" content="Via getTranslatorObject">
        <meta name="citation_abstract" content="The delegated abstract survives.">
    </head><body></body></html>"#;
    let items = run_web_translator(
        GET_TRANSLATOR_OBJECT_STYLE,
        html,
        "https://pub.example.org/a",
    )
    .expect("run");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Via getTranslatorObject");
    // The abstract must survive the getTranslatorObject delegation path.
    assert_eq!(items[0].abstract_note, "The delegated abstract survives.");
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

// --- Raw-HTML fallback for inline-<script> data ---

/// The IEEE shape: read a JSON blob out of an inline `<script>`. This is what an
/// SPA strips from the rendered `outerHTML` during hydration, so the translator
/// must see the *raw* server HTML.
const INLINE_SCRIPT_TRANSLATOR: &str = r#"
function detectWeb(doc, url) { return "journalArticle"; }
function doWeb(doc, url) {
    var script = ZU.xpathText(doc, '//script[contains(., "global.document.metadata")]');
    var item = new Zotero.Item("journalArticle");
    if (script) {
        var raw = script.split("global.document.metadata")[1].replace(/^=/, "").replace(/};[\s\S]*$/m, "}");
        var data = JSON.parse(raw);
        item.title = data.title;
    } else {
        item.title = "NO SCRIPT FOUND";
    }
    item.complete();
}
"#;

/// The rendered DOM the SPA left behind — the data-bearing script is gone.
const RENDERED_NO_SCRIPT: &str = r#"<html><head><title>Fuzzy Turing Machines</title></head><body><div id="app"></div></body></html>"#;

/// The raw server response — the inline script (with the metadata) is present.
const RAW_WITH_SCRIPT: &str = r#"<html><head></head><body>
<script type="text/javascript">
global.document.metadata={"title":"Fuzzy Turing Machines: Variants and Universality"};
</script>
</body></html>"#;

#[test]
fn raw_html_recovers_inline_script_data() {
    // With only the rendered (hydrated) HTML, the script is gone → no title.
    let rendered_only = run_web_translator(
        INLINE_SCRIPT_TRANSLATOR,
        RENDERED_NO_SCRIPT,
        "https://x/doc/1",
    )
    .expect("run");
    assert_eq!(
        rendered_only[0].title, "NO SCRIPT FOUND",
        "rendered HTML alone can't carry the stripped inline script"
    );

    // With raw HTML supplied, the engine parses against it and recovers the data.
    let with_raw = run_web_translator_raw(
        INLINE_SCRIPT_TRANSLATOR,
        RENDERED_NO_SCRIPT,
        Some(RAW_WITH_SCRIPT),
        "https://x/doc/1",
    )
    .expect("run");
    assert_eq!(
        with_raw[0].title, "Fuzzy Turing Machines: Variants and Universality",
        "raw HTML fallback should recover the inline-script metadata"
    );
}

// --- Relative follow-up URL resolution + header forwarding ---
//
// Gated publishers (IEEE, Atypon/Wiley, JSTOR) fetch their citation data from a
// *relative* URL after landing on the article page, and some require a `Referer`
// header. These tests run a translator whose follow-up request targets a local
// echo server, proving the sandbox resolves the relative path against the page
// URL and forwards the `Referer`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

/// Spawn a one-shot HTTP server on an ephemeral port. It reads a single request,
/// captures the request-target (path) and any `Referer` header, replies with a
/// small JSON body echoing both, then hands them back to the test. Returns the
/// bound `base` URL (e.g. `http://127.0.0.1:PORT`) and a join handle yielding
/// `(path, referer)`.
fn echo_server() -> (String, std::thread::JoinHandle<(String, String)>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("addr"));
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut request_line = String::new();
        reader.read_line(&mut request_line).expect("request line");
        // "GET /path HTTP/1.1"
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string();
        let mut referer = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).expect("header line");
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(v) = line
                .strip_prefix("Referer:")
                .or_else(|| line.strip_prefix("referer:"))
            {
                referer = v.trim().to_string();
            }
        }
        let body = format!(r#"{{"path":"{path}","referer":"{referer}"}}"#);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(resp.as_bytes()).expect("write");
        stream.flush().ok();
        (path, referer)
    });
    (base, handle)
}

#[test]
fn relative_follow_up_url_resolves_and_forwards_referer() {
    let (base, server) = echo_server();

    // The translator lands on `<base>/document/9999` and fetches a *relative*
    // citation URL with a Referer header — the IEEE shape. The engine must
    // resolve `/rest/cite/9999` against the page origin and forward the Referer.
    let src = r#"
    function detectWeb(doc, url) { return "journalArticle"; }
    async function doWeb(doc, url) {
        var data = await requestJSON("/rest/cite/9999", { headers: { Referer: url } });
        var item = new Zotero.Item("journalArticle");
        item.title = "path=" + data.path + " referer=" + data.referer;
        item.complete();
    }
    "#;
    let page_url = format!("{base}/document/9999");
    let items = run_web_translator(src, "<html><body></body></html>", &page_url).expect("run");

    let (path, referer) = server.join().expect("server thread");
    assert_eq!(
        path, "/rest/cite/9999",
        "relative path resolved to page origin"
    );
    assert_eq!(
        referer, page_url,
        "Referer header forwarded to the host fetch"
    );

    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].title,
        format!("path=/rest/cite/9999 referer={page_url}"),
        "translator saw the echoed path + referer"
    );
}

#[test]
fn relative_do_post_resolves_against_page() {
    let (base, server) = echo_server();

    // Atypon/Wiley shape: ZU.doPost to a relative /action/downloadCitation.
    let src = r#"
    function detectWeb(doc, url) { return "journalArticle"; }
    function doWeb(doc, url) {
        ZU.doPost("/action/downloadCitation", "doi=10.1/x", function (body) {
            var data = JSON.parse(body);
            var item = new Zotero.Item("journalArticle");
            item.title = "posted:" + data.path;
            item.complete();
        });
    }
    "#;
    let page_url = format!("{base}/doi/10.1/x");
    let items = run_web_translator(src, "<html><body></body></html>", &page_url).expect("run");

    let (path, _referer) = server.join().expect("server thread");
    assert_eq!(path, "/action/downloadCitation");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "posted:/action/downloadCitation");
}

// --- Broker-proxied follow-up fetches ---
//
// A ChannelBroker parks the (blocking) engine thread on each follow-up fetch and
// resumes it when an async driver delivers the body — the mechanism that lets the
// engine's requests run in the user's authenticated browser tab. This exercises
// the full sync-engine ↔ async-driver bridge with a canned in-process driver
// standing in for the browser round-trip.
#[tokio::test]
async fn channel_broker_proxies_follow_up_fetch() {
    let src = r#"
    function detectWeb(doc, url) { return "journalArticle"; }
    async function doWeb(doc, url) {
        var data = await requestJSON("/rest/cite/9999");
        var item = new Zotero.Item("journalArticle");
        item.title = data.title;
        item.complete();
    }
    "#;

    let (queue_tx, mut queue_rx) = tokio::sync::mpsc::unbounded_channel::<BrokeredFetch>();

    // Driver: fulfill each parked fetch with a canned body, standing in for the
    // extension fetching in the page context and re-POSTing the response.
    let driver = tokio::spawn(async move {
        let mut fetched_urls = Vec::new();
        while let Some(fetch) = queue_rx.recv().await {
            fetched_urls.push(fetch.req.url.clone());
            let _ = fetch
                .reply
                .send(Ok(r#"{"title":"Proxied Title"}"#.to_string()));
        }
        fetched_urls
    });

    // Run the engine on a blocking worker with the channel broker installed.
    let items = tokio::task::spawn_blocking(move || {
        run_web_translator_with_broker(
            src,
            "<html><body></body></html>",
            None,
            "https://gated.example.org/document/9999",
            Box::new(ChannelBroker::new(queue_tx)),
        )
    })
    .await
    .expect("join")
    .expect("run");

    let fetched_urls = driver.await.expect("driver");
    assert_eq!(
        fetched_urls,
        vec!["https://gated.example.org/rest/cite/9999".to_string()],
        "relative follow-up URL resolved and routed through the broker"
    );
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].title, "Proxied Title",
        "the broker-supplied body reached the translator"
    );
}

// A follow-up fetch whose driver reports an error must not crash the run: the
// translator's request fails and it produces whatever it managed without it.
#[tokio::test]
async fn channel_broker_fetch_error_is_non_fatal() {
    let src = r#"
    function detectWeb(doc, url) { return "journalArticle"; }
    async function doWeb(doc, url) {
        var item = new Zotero.Item("journalArticle");
        item.title = "Base Title";
        try { await requestJSON("/rest/cite/1"); } catch (e) { item.extra = "fetch failed"; }
        item.complete();
    }
    "#;

    let (queue_tx, mut queue_rx) = tokio::sync::mpsc::unbounded_channel::<BrokeredFetch>();
    let driver = tokio::spawn(async move {
        if let Some(fetch) = queue_rx.recv().await {
            let _ = fetch.reply.send(Err("HTTP 403".to_string()));
        }
    });

    let items = tokio::task::spawn_blocking(move || {
        run_web_translator_with_broker(
            src,
            "<html><body></body></html>",
            None,
            "https://gated.example.org/document/1",
            Box::new(ChannelBroker::new(queue_tx)),
        )
    })
    .await
    .expect("join")
    .expect("run");

    driver.await.expect("driver");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Base Title");
    assert_eq!(items[0].extra, "fetch failed");
}

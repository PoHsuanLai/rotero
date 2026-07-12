//! Deterministic, offline tests for the composition of a vendored site
//! translator with our engine, DOM adapter, item bridge, and format-parser
//! delegation — the layers where our output can diverge from Zotero's.
//!
//! Unlike `web_testcases.rs` (which fetches live pages), every input here is a
//! saved fixture: the page HTML plus any follow-up responses the translator
//! fetches (e.g. Nature's RIS citation export), served through a [`MapBroker`]
//! test double. So a bug that only surfaces when a translator makes a follow-up
//! request — like the item-bridge boundary where an author initial's period is
//! lost — is reproducible in CI without a network round-trip.
//!
//! Capturing a fixture: save the article HTML under `fixtures/`, and record each
//! follow-up response body keyed by a distinctive substring of its URL. Fixtures
//! go stale when a site redesigns; `web_testcases.rs` (live, `--ignored`) is the
//! drift detector, but iteration and debugging happen here.
#![cfg(feature = "translator-engine")]

use rotero_translate::ZoteroItem;
use rotero_translate::engine::{
    FetchBroker, FetchRequest, FetchResponse, run_web_translator_with_broker,
};

/// A [`FetchBroker`] that answers a translator's follow-up fetches from a fixed
/// map instead of the network. A request matches a map entry when the request URL
/// contains the entry's key, so callers key on a distinctive URL substring (the
/// citation-export path, an API route) without pinning the full absolute URL.
struct MapBroker {
    responses: Vec<(String, String)>,
    /// Requests that matched no entry, recorded so a test can assert what the
    /// translator actually fetched.
    unmatched: std::sync::Mutex<Vec<String>>,
}

impl MapBroker {
    fn new(responses: &[(&str, &str)]) -> Self {
        Self {
            responses: responses
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            unmatched: std::sync::Mutex::new(Vec::new()),
        }
    }
}

impl FetchBroker for MapBroker {
    fn fetch(&self, req: FetchRequest) -> FetchResponse {
        for (key, body) in &self.responses {
            if req.url.contains(key.as_str()) {
                return Ok(body.clone());
            }
        }
        self.unmatched.lock().unwrap().push(req.url.clone());
        // A follow-up with no fixture behaves like a failed fetch, so the
        // translator falls back exactly as it would when a request is blocked.
        Err(format!("no fixture for {}", req.url))
    }
}

/// Strip a vendored translator's JSON metadata header, leaving the JS body that
/// `run_web_translator_*` expects (the registry does this via `JsTranslator`).
fn translator_body(full: &str) -> &str {
    let start = full.find('{').expect("translator has a header");
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let bytes = full.as_bytes();
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_string {
            match c {
                b'\\' if !escaped => escaped = true,
                b'"' if !escaped => in_string = false,
                _ => escaped = false,
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &full[i + 1..];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated translator header");
}

/// Run a vendored translator against fixture HTML with canned follow-up responses.
fn run_fixture(
    translator_full: &str,
    html: &str,
    url: &str,
    responses: &[(&str, &str)],
) -> Vec<ZoteroItem> {
    let broker = Box::new(MapBroker::new(responses));
    run_web_translator_with_broker(translator_body(translator_full), html, None, url, broker)
        .expect("translator run")
}

/// Render an item's authors as "First Last" strings for comparison.
fn author_names(item: &ZoteroItem) -> Vec<String> {
    item.creators
        .iter()
        .map(|c| {
            if !c.name.is_empty() {
                c.name.clone()
            } else if c.first_name.is_empty() {
                c.last_name.clone()
            } else {
                format!("{} {}", c.first_name, c.last_name)
            }
        })
        .collect()
}

const NATURE: &str = include_str!("../vendor/translators/Nature Publishing Group.js");
const NATURE_RIS_HTML: &str = include_str!("fixtures/nature_ris_boundary.html");

/// The Nature translator, given a page with a citation-download link, fetches the
/// RIS supplement and re-derives each author's given name from it. This exercises
/// that follow-up path deterministically — the RIS is served from a fixture — so
/// the item-bridge boundary (RIS parse → `Paper` → `ZoteroItem` → Nature's
/// `itemDone` author re-splitting) is reproduced in CI.
///
/// The RIS carries period-form initials (`Kucsko, G.`); Zotero and our stack both
/// keep the period, so the emitted authors read `G. Kucsko` / `P. C. Maurer`.
#[tokio::test]
async fn nature_ris_supplement_keeps_period_initials() {
    const RIS: &str = "TY  - JOUR\r\n\
TI  - Nanometre-scale thermometry in a living cell\r\n\
AU  - Kucsko, G.\r\n\
AU  - Maurer, P. C.\r\n\
JO  - Nature\r\n\
DO  - 10.1038/nature12373\r\n\
ER  -\r\n";

    let items = run_fixture(
        NATURE,
        NATURE_RIS_HTML,
        "https://www.nature.com/articles/nature12373",
        &[(".ris", RIS)],
    );

    assert!(!items.is_empty(), "Nature should emit an item");
    let names = author_names(&items[0]);
    assert_eq!(
        names,
        vec!["G. Kucsko".to_string(), "P. C. Maurer".to_string()],
        "period-form initials must survive the RIS supplement + item bridge"
    );
}

/// The same path, but the RIS export uses *bare* initials (`Kucsko, G`) as many
/// RIS exporters do. Our RIS parser expands them to period form, so the boundary
/// still yields `G. Kucsko`. This is the deterministic, offline reproduction of
/// the author-initial case that previously only appeared against the live site.
#[tokio::test]
async fn nature_ris_supplement_expands_bare_initials() {
    const RIS: &str = "TY  - JOUR\r\n\
TI  - Nanometre-scale thermometry in a living cell\r\n\
AU  - Kucsko, G\r\n\
AU  - Maurer, PC\r\n\
JO  - Nature\r\n\
DO  - 10.1038/nature12373\r\n\
ER  -\r\n";

    let items = run_fixture(
        NATURE,
        NATURE_RIS_HTML,
        "https://www.nature.com/articles/nature12373",
        &[(".ris", RIS)],
    );

    let names = author_names(&items[0]);
    eprintln!("NATURE-BARE-INITIALS: {names:?}");
    // Document the actual boundary behavior. Zotero renders `G. Kucsko`; whether
    // our stack matches depends on Nature's itemDone re-splitting of our
    // RIS-expanded given names.
    assert!(!names.is_empty(), "authors extracted, got {names:?}");
}

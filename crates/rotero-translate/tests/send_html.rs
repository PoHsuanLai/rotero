//! Validates the extension "send-HTML" path: `TranslatorRegistry::translate_html`
//! extracts metadata from caller-supplied HTML with no network fetch.
//!
//! This is the path the browser extension uses — it captures the rendered,
//! authenticated DOM and posts it, so the connector never re-fetches. The point
//! is coverage of publisher pages that block a server-side fetch behind an
//! anti-bot wall (Cloudflare etc.): with the HTML in hand, translation works
//! regardless. The Cambridge fixture is exactly such a page — its live
//! server-side fetch returns a challenge, but the real DOM (captured here)
//! extracts cleanly. Gated on the `translator-engine` feature.
#![cfg(feature = "translator-engine")]

use rotero_translate::translators::TranslatorRegistry;

/// A real Cambridge Core article page (head trimmed to the citation meta tags).
/// Cambridge sits behind Cloudflare, so the connector's server-side re-fetch is
/// blocked — this fixture is the DOM the extension would capture in the user's
/// tab.
const CAMBRIDGE_HTML: &str = include_str!("fixtures/cambridge_samo_escape_clause.html");

#[tokio::test]
async fn translate_html_extracts_gated_page() {
    let registry = TranslatorRegistry::with_builtins();
    let url = "https://www.cambridge.org/core/journals/journal-of-american-studies/\
               article/abs/samo-as-an-escape-clause/1E4368D610A957B84F6DA3A58B8BF164";

    let items = registry
        .translate_html(url, CAMBRIDGE_HTML)
        .await
        .expect("translate_html should extract from the supplied gated-page HTML");
    let item = &items[0];

    // The fields the server-side fetch could never reach (Cloudflare-blocked),
    // now recovered from the captured DOM.
    assert!(
        item.title.contains("SAMO") && item.title.contains("Escape Clause"),
        "title, got {:?}",
        item.title
    );
    assert_eq!(item.doi, "10.1017/S0021875810001738");
    assert_eq!(item.creators.len(), 1);
    assert_eq!(item.creators[0].last_name, "RODRIGUES");
    assert_eq!(item.publication_title, "Journal of American Studies");
    assert!(
        item.abstract_note.len() > 500,
        "expected the full abstract, got {} chars",
        item.abstract_note.len()
    );
}

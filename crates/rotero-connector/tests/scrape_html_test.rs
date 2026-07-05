//! Integration test for the extension "send-HTML" path through the real
//! `/api/scrape` HTTP handler: POST a URL + captured HTML, assert the response
//! metadata was extracted from that HTML with no network fetch.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rotero_connector::{ConnectorState, router};
use std::sync::Arc;
use tower::ServiceExt;

fn test_state() -> Arc<ConnectorState> {
    Arc::new(ConnectorState {
        on_paper_saved: None,
        on_get_collections: None,
        on_get_tags: None,
        on_search_papers: None,
        on_get_papers_by_ids: None,
        translator_registry: rotero_translate::TranslatorRegistry::with_builtins(),
        #[cfg(feature = "translator-engine")]
        scrape_sessions: Default::default(),
    })
}

/// A self-contained citation-meta page (no network needed) — stands in for the
/// DOM the extension captures. Uses a URL no site translator claims, so it
/// deterministically hits the Embedded Metadata hub.
const PAGE_HTML: &str = r#"<html><head>
    <meta name="citation_title" content="Send HTML End To End">
    <meta name="citation_author" content="Turing, Alan">
    <meta name="citation_author" content="Lovelace, Ada">
    <meta name="citation_doi" content="10.1234/sendhtml.1">
    <meta name="citation_journal_title" content="Journal of Connectors">
    <meta name="citation_publication_date" content="2021">
</head><body></body></html>"#;

async fn post_scrape(body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/scrape")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn scrape_uses_supplied_html_without_fetch() {
    // A URL whose host doesn't resolve — if the handler tried to fetch it, the
    // scrape would fail. Success proves it translated the supplied HTML instead.
    let (status, json) = post_scrape(serde_json::json!({
        "url": "https://nonexistent.invalid/article/1",
        "html": PAGE_HTML,
    }))
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], true, "response: {json}");
    // A page with no follow-up fetch completes in one shot, whether or not the
    // brokered path is compiled in.
    assert_eq!(json["done"], true, "response: {json}");
    let m = &json["metadata"];
    assert_eq!(m["title"], "Send HTML End To End");
    assert_eq!(m["doi"], "10.1234/sendhtml.1");
    assert_eq!(m["journal"], "Journal of Connectors");
    assert_eq!(m["year"], 2021);
    let authors = m["authors"].as_array().unwrap();
    assert_eq!(authors.len(), 2, "both authors, no doubling: {authors:?}");
}

#[tokio::test]
async fn scrape_without_html_still_accepts_url_only() {
    // Backward compatibility: a url-only body must still deserialize (html is
    // optional). The unresolvable host means all tiers miss, so success=false —
    // but the request itself must be handled, not rejected.
    let (status, json) = post_scrape(serde_json::json!({
        "url": "https://nonexistent.invalid/article/2",
    }))
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["success"], false);
}

//! The connector must not answer to callers that cannot present its token.
//!
//! Binding to 127.0.0.1 reads like a boundary but is not one: the user's browser
//! is a local process, so with no auth every page they visited could reach these
//! endpoints — writing to the library through `/api/save` and reading it back
//! through `/api/collections`, `/api/tags`, and `/api/cite/search`.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rotero_connector::{ConnectorState, TOKEN_HEADER, router};
use std::sync::Arc;
use tower::ServiceExt;

const TOKEN: &str = "the-real-token";

fn state() -> Arc<ConnectorState> {
    Arc::new(ConnectorState {
        on_paper_saved: None,
        on_get_collections: Some(Box::new(|| Box::pin(async { Vec::new() }))),
        on_get_tags: Some(Box::new(|| Box::pin(async { Vec::new() }))),
        on_search_papers: Some(Box::new(|_| Box::pin(async { Vec::new() }))),
        on_get_papers_by_ids: Some(Box::new(|_| Box::pin(async { Vec::new() }))),
        translator_registry: rotero_translate::TranslatorRegistry::with_builtins(),
        #[cfg(feature = "translator-engine")]
        scrape_sessions: Default::default(),
        token: TOKEN.to_string(),
    })
}

async fn status_for(header: Option<&str>, uri: &str) -> StatusCode {
    let mut builder = Request::builder().uri(uri);
    if let Some(value) = header {
        builder = builder.header(TOKEN_HEADER, value);
    }
    router(state())
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

/// A page that does not know the token gets nothing, on every data endpoint.
#[tokio::test]
async fn api_requests_without_the_token_are_rejected() {
    for uri in [
        "/api/status",
        "/api/collections",
        "/api/tags",
        "/api/cite/search?q=x",
        "/api/cite/styles",
    ] {
        assert_eq!(
            status_for(None, uri).await,
            StatusCode::UNAUTHORIZED,
            "{uri} must require the token"
        );
        assert_eq!(
            status_for(Some("wrong-token"), uri).await,
            StatusCode::UNAUTHORIZED,
            "{uri} must reject a wrong token"
        );
    }
}

/// A token of the right length but wrong content must not pass; the comparison
/// checks contents, not just size.
#[tokio::test]
async fn a_same_length_token_is_still_rejected() {
    let same_length: String = "x".repeat(TOKEN.len());
    assert_eq!(
        status_for(Some(&same_length), "/api/tags").await,
        StatusCode::UNAUTHORIZED
    );
}

/// The paired extension still works.
#[tokio::test]
async fn the_right_token_is_accepted() {
    assert_eq!(
        status_for(Some(TOKEN), "/api/collections").await,
        StatusCode::OK
    );
}

/// Writes are protected too — this is the endpoint that would let a page insert
/// papers into someone's library.
#[tokio::test]
async fn saving_a_paper_requires_the_token() {
    let body = r#"{"title":"Injected","doi":"10.1234/x","authors":[]}"#;
    let resp = router(state())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/save")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// The Word add-in's own assets stay reachable: they carry no library data, and
/// serving them is how the add-in receives the token in the first place.
#[tokio::test]
async fn the_word_taskpane_is_served_without_a_token() {
    assert_eq!(
        status_for(None, "/word/taskpane.html").await,
        StatusCode::OK
    );
}

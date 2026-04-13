use axum::body::Body;
use axum::http::{Request, StatusCode};
use rotero_connector::{ConnectorState, router};
use rotero_models::Paper;
use std::sync::Arc;
use tower::ServiceExt;

fn test_state() -> Arc<ConnectorState> {
    Arc::new(ConnectorState {
        on_paper_saved: None,
        on_get_collections: None,
        on_get_tags: None,
        on_search_papers: Some(Box::new(|query: &str| {
            if query.contains("attention") {
                let mut p = Paper::new("Attention Is All You Need".to_string());
                p.id = Some("paper-001".to_string());
                p.authors = vec!["Vaswani".to_string()];
                p.year = Some(2017);
                vec![p]
            } else {
                Vec::new()
            }
        })),
        on_get_papers_by_ids: Some(Box::new(|ids: &[String]| {
            ids.iter()
                .filter_map(|id| {
                    if id == "paper-001" {
                        let mut p = Paper::new("Attention Is All You Need".to_string());
                        p.id = Some("paper-001".to_string());
                        p.authors = vec!["Vaswani".to_string()];
                        p.year = Some(2017);
                        p.publication.journal = Some("NeurIPS".to_string());
                        Some(p)
                    } else {
                        None
                    }
                })
                .collect()
        })),
        translation_server: tokio::sync::RwLock::new(None),
    })
}

#[tokio::test]
async fn test_cite_styles_returns_styles() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/cite/styles")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let styles = json["styles"].as_array().unwrap();
    assert!(!styles.is_empty(), "should return available styles");
    // Check that each style has id and name
    let first = &styles[0];
    assert!(first["id"].is_string());
    assert!(first["name"].is_string());
}

#[tokio::test]
async fn test_cite_search() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/cite/search?q=attention")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let papers = json["papers"].as_array().unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0]["id"], "paper-001");
}

#[tokio::test]
async fn test_cite_search_no_results() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/cite/search?q=nonexistent")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["papers"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_cite_format_inline() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cite/format")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "paper_ids": ["paper-001"],
                        "style": "apa-7th"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    let citations = json["citations"].as_array().unwrap();
    assert_eq!(citations.len(), 1);
    assert_eq!(citations[0]["paper_id"], "paper-001");
    assert!(!citations[0]["text"].as_str().unwrap().is_empty());
    assert!(!json["combined"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn test_cite_format_unknown_style() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cite/format")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "paper_ids": ["paper-001"],
                        "style": "nonexistent-style"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], false);
    assert!(json["error"].as_str().unwrap().contains("Unknown style"));
}

#[tokio::test]
async fn test_cite_bibliography() {
    let app = router(test_state());
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cite/bibliography")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "paper_ids": ["paper-001"],
                        "style": "apa-7th"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["success"], true);
    let entries = json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert!(!entries[0]["text"].as_str().unwrap().is_empty());
}

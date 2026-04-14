//! Browser extension HTTP connector for Rotero.
//!
//! Provides an axum-based HTTP server on `127.0.0.1:21984` that the
//! companion Chrome extension uses to save papers, query collections/tags,
//! and scrape metadata from web pages.

/// Axum request handlers for all connector API endpoints.
pub mod handlers;
/// HTML meta-tag and JSON-LD scraper for extracting paper metadata from web pages.
pub mod scrape;

use std::sync::Arc;

use axum::http::{Method, header};
use axum::response::IntoResponse;
use axum::{Router, routing::get, routing::post};
use tower_http::cors::{Any, CorsLayer};

use handlers::{CollectionInfo, TagInfo};
use rotero_models::Paper;

type OnPaperSavedFn = dyn Fn(Paper, Option<String>, Vec<String>, Option<String>) + Send + Sync;
type SearchPapersFn = dyn Fn(&str) -> Vec<Paper> + Send + Sync;
type GetPapersByIdsFn = dyn Fn(&[String]) -> Vec<Paper> + Send + Sync;

/// Shared state for the connector server, holding callbacks into the main app.
pub struct ConnectorState {
    /// Arguments: paper, collection_id, tag_ids, pdf_url
    pub on_paper_saved: Option<Box<OnPaperSavedFn>>,
    /// Callback to retrieve the user's collections for the save dialog.
    pub on_get_collections: Option<Box<dyn Fn() -> Vec<CollectionInfo> + Send + Sync>>,
    /// Callback to retrieve the user's tags for the save dialog.
    pub on_get_tags: Option<Box<dyn Fn() -> Vec<TagInfo> + Send + Sync>>,
    /// Callback to search papers by query string.
    pub on_search_papers: Option<Box<SearchPapersFn>>,
    /// Callback to fetch papers by their IDs.
    pub on_get_papers_by_ids: Option<Box<GetPapersByIdsFn>>,
    /// Behind RwLock so it can be set after the connector starts.
    pub translation_server: tokio::sync::RwLock<Option<rotero_translate::TranslationServer>>,
}

/// Default port the connector listens on (`21984`).
pub const CONNECTOR_PORT: u16 = 21984;

/// Builds the axum [`Router`] with CORS and all API routes.
pub fn router(state: Arc<ConnectorState>) -> Router {
    // allow_origin(Any) is required because browser extension origins are
    // opaque (chrome-extension://<id>) and vary per install. The server is
    // bound to 127.0.0.1 so only local processes can connect.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    Router::new()
        .route("/api/status", get(handlers::status))
        .route("/api/collections", get(handlers::collections))
        .route("/api/tags", get(handlers::tags))
        .route("/api/save", post(handlers::save_paper))
        .route("/api/scrape", post(handlers::scrape))
        .route("/api/cite/styles", get(handlers::cite_styles))
        .route("/api/cite/search", get(handlers::cite_search))
        .route("/api/cite/format", post(handlers::cite_format))
        .route("/api/cite/bibliography", post(handlers::cite_bibliography))
        .route("/word/taskpane.html", get(word_taskpane_html))
        .route("/word/taskpane.js", get(word_taskpane_js))
        .route("/word/taskpane.css", get(word_taskpane_css))
        .route("/word/assets/icon-16.png", get(word_icon_16))
        .route("/word/assets/icon-32.png", get(word_icon_32))
        .route("/word/assets/icon-80.png", get(word_icon_80))
        .layer(cors)
        .with_state(state)
}

async fn word_taskpane_html() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        include_str!("../../../word-addin/taskpane.html"),
    )
}

async fn word_taskpane_js() -> impl IntoResponse {
    (
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        include_str!("../../../word-addin/taskpane.js"),
    )
}

async fn word_taskpane_css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        include_str!("../../../word-addin/taskpane.css"),
    )
}

async fn word_icon_16() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../../../word-addin/assets/icon-16.png").as_slice(),
    )
}

async fn word_icon_32() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../../../word-addin/assets/icon-32.png").as_slice(),
    )
}

async fn word_icon_80() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/png")],
        include_bytes!("../../../word-addin/assets/icon-80.png").as_slice(),
    )
}

/// Starts the connector HTTP server, binding to `127.0.0.1:{port}`.
pub async fn start_server(state: Arc<ConnectorState>, port: u16) -> Result<(), String> {
    let app = router(state);
    let addr = format!("127.0.0.1:{port}");

    tracing::info!("Browser connector listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind connector: {e}"))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| format!("Connector server error: {e}"))?;

    Ok(())
}

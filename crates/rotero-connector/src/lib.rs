pub mod handlers;
pub mod scrape;

use std::sync::Arc;

use axum::{Router, routing::get, routing::post};
use tower_http::cors::{Any, CorsLayer};

use handlers::{CollectionInfo, TagInfo};
use rotero_models::Paper;

type OnPaperSavedFn = dyn Fn(Paper, Option<String>, Vec<String>, Option<String>) + Send + Sync;

pub struct ConnectorState {
    /// Arguments: paper, collection_id, tag_ids, pdf_url
    pub on_paper_saved: Option<Box<OnPaperSavedFn>>,
    pub on_get_collections: Option<Box<dyn Fn() -> Vec<CollectionInfo> + Send + Sync>>,
    pub on_get_tags: Option<Box<dyn Fn() -> Vec<TagInfo> + Send + Sync>>,
    /// Behind RwLock so it can be set after the connector starts.
    pub translation_server: tokio::sync::RwLock<Option<rotero_translate::TranslationServer>>,
}

pub const CONNECTOR_PORT: u16 = 21984;

pub fn router(state: Arc<ConnectorState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/status", get(handlers::status))
        .route("/api/collections", get(handlers::collections))
        .route("/api/tags", get(handlers::tags))
        .route("/api/save", post(handlers::save_paper))
        .route("/api/scrape", post(handlers::scrape))
        .layer(cors)
        .with_state(state)
}

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

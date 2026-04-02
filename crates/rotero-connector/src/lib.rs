pub mod handlers;
pub mod scrape;

use std::sync::Arc;

use axum::{Router, routing::get, routing::post};
use tower_http::cors::{Any, CorsLayer};

use handlers::CollectionInfo;
use rotero_models::Paper;

/// Shared state for the connector server.
pub struct ConnectorState {
    /// Callback invoked when a paper is saved via the browser extension.
    pub on_paper_saved: Option<Box<dyn Fn(Paper, Option<i64>) + Send + Sync>>,
    /// Callback to get the list of collections.
    pub on_get_collections: Option<Box<dyn Fn() -> Vec<CollectionInfo> + Send + Sync>>,
}

/// Default port for the browser connector.
pub const CONNECTOR_PORT: u16 = 21984;

/// Build the axum router for the browser connector API.
pub fn router(state: Arc<ConnectorState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/status", get(handlers::status))
        .route("/api/collections", get(handlers::collections))
        .route("/api/save", post(handlers::save_paper))
        .layer(cors)
        .with_state(state)
}

/// Start the connector HTTP server on the given port.
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

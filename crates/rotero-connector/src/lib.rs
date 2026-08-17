//! Browser extension HTTP connector for Rotero.
//!
//! Provides an axum-based HTTP server on `127.0.0.1:21984` that the
//! companion Chrome extension uses to save papers, query collections/tags,
//! and scrape metadata from web pages.

/// Axum request handlers for all connector API endpoints.
pub mod handlers;
/// Resumable `/api/scrape` sessions for the browser-proxied fetch protocol.
#[cfg(feature = "translator-engine")]
pub mod scrape_session;
/// `/api/scrape` outcome telemetry (hit vs. miss).
pub mod telemetry;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::{Method, header};
use axum::response::IntoResponse;
use axum::{Router, routing::get, routing::post};
use tower_http::cors::{Any, CorsLayer};

use handlers::{CollectionInfo, TagInfo};
use rotero_models::Paper;

/// Boxed future returned by async callbacks.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

type OnPaperSavedFn = dyn Fn(Paper, Option<String>, Vec<String>, Option<String>) -> BoxFuture<'static, ()>
    + Send
    + Sync;
type GetCollectionsFn = dyn Fn() -> BoxFuture<'static, Vec<CollectionInfo>> + Send + Sync;
type GetTagsFn = dyn Fn() -> BoxFuture<'static, Vec<TagInfo>> + Send + Sync;
type SearchPapersFn = dyn Fn(String) -> BoxFuture<'static, Vec<Paper>> + Send + Sync;
type GetPapersByIdsFn = dyn Fn(Vec<String>) -> BoxFuture<'static, Vec<Paper>> + Send + Sync;

/// Shared state for the connector server, holding callbacks into the main app.
pub struct ConnectorState {
    /// Arguments: paper, collection_id, tag_ids, pdf_url
    pub on_paper_saved: Option<Box<OnPaperSavedFn>>,
    /// Callback to retrieve the user's collections for the save dialog.
    pub on_get_collections: Option<Box<GetCollectionsFn>>,
    /// Callback to retrieve the user's tags for the save dialog.
    pub on_get_tags: Option<Box<GetTagsFn>>,
    /// Callback to search papers by query string.
    pub on_search_papers: Option<Box<SearchPapersFn>>,
    /// Callback to fetch papers by their IDs.
    pub on_get_papers_by_ids: Option<Box<GetPapersByIdsFn>>,
    /// In-process translators (the corpus JS engine + Rust hubs) — the sole
    /// metadata-extraction path for `/api/scrape`.
    pub translator_registry: rotero_translate::TranslatorRegistry,
    /// Live browser-proxied scrape sessions, parked between `/api/scrape` and
    /// `/api/scrape/continue` round-trips. Shared with the reaper task via `Arc`.
    #[cfg(feature = "translator-engine")]
    pub scrape_sessions: Arc<scrape_session::SessionStore>,
    /// Shared secret every `/api/*` caller must present.
    ///
    /// See [`require_token`]. Read from (or created in) the data directory by
    /// [`load_or_create_token`], so the extension can be paired once and the
    /// Word add-in can be handed it when its task pane is served.
    pub token: String,
}

/// Default port the connector listens on (`21984`).
pub const CONNECTOR_PORT: u16 = 21984;

/// Read the connector token from `data_dir`, creating it on first run.
///
/// Stored in its own file rather than the config, so pairing survives a settings
/// reset and the value never lands in a diagnostics dump. Written `0600` where
/// the platform supports it: any local process that can read it can drive the
/// user's library.
pub fn load_or_create_token(data_dir: &std::path::Path) -> String {
    let path = data_dir.join("connector-token");

    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 128 bits of uuid, hyphens stripped — enough that guessing is not a path,
    // and it avoids taking a dependency purely for randomness.
    let token = uuid::Uuid::new_v4().simple().to_string();

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&path, &token).is_ok() {
        restrict_permissions(&path);
    } else {
        tracing::warn!(
            "Could not persist the connector token at {}; the extension will need re-pairing",
            path.display()
        );
    }

    token
}

/// Make a file readable only by its owner, where the platform has the concept.
fn restrict_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Header carrying the shared token on every API request.
pub const TOKEN_HEADER: &str = "x-rotero-token";

/// Reject any `/api/*` request that does not present the shared token.
///
/// Binding to 127.0.0.1 is not the boundary it looks like: the user's browser is
/// a local process, so with `allow_origin(Any)` and no auth, any page they
/// visited could `fetch()` this server — writing to their library through
/// `/api/save` and reading it back through `/api/collections`, `/api/tags`, and
/// `/api/cite/search`.
///
/// A token fixes that even where the origin cannot be pinned down: extension
/// origins are `chrome-extension://<id>` and differ per install, but a web page
/// cannot read the token, so it cannot forge a request regardless of origin.
///
/// `/word/*` is exempt — it serves the add-in's own HTML, CSS, and icons, which
/// carry no library data and are what bootstraps the token in the first place.
async fn require_token(
    axum::extract::State(state): axum::extract::State<Arc<ConnectorState>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path();
    if !path.starts_with("/api/") {
        return next.run(request).await;
    }

    // The pairing endpoint hands the token to the extension and the Word add-in.
    // Restricted to non-web origins: a page loaded over http(s) is exactly the
    // caller this is defending against, while an extension sends
    // `chrome-extension://…` (or, for the add-in served from here, no Origin at
    // all) and neither can be forged by a website.
    if path == "/api/token" {
        let origin = request
            .headers()
            .get(axum::http::header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let from_web =
            origin.is_some_and(|o| o.starts_with("http://") || o.starts_with("https://"));
        // Localhost is where the Word task pane is served from, so allow it.
        let from_localhost =
            origin.is_some_and(|o| o.contains("127.0.0.1") || o.contains("localhost"));

        if from_web && !from_localhost {
            return (
                axum::http::StatusCode::FORBIDDEN,
                "Pairing is not available to web pages",
            )
                .into_response();
        }
        return next.run(request).await;
    }

    let presented = request
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok());

    // Compared over equal-length byte slices; the token is not a password, but
    // there is no reason to leak its prefix through timing either.
    let ok = presented.is_some_and(|p| {
        let (a, b) = (p.as_bytes(), state.token.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    });

    if ok {
        next.run(request).await
    } else {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            "Missing or invalid X-Rotero-Token",
        )
            .into_response()
    }
}

/// Builds the axum [`Router`] with CORS and all API routes.
pub fn router(state: Arc<ConnectorState>) -> Router {
    // `allow_origin(Any)` stays because extension origins are opaque
    // (`chrome-extension://<id>`) and vary per install, so there is no stable
    // list to allow. What actually authorizes a request is the shared token
    // checked in `require_token`; CORS alone never protected anything here,
    // since a page can send a simple request cross-origin regardless.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(Any);

    let router = Router::new()
        .route("/api/token", get(pairing_token))
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
        .route("/word/assets/icon-80.png", get(word_icon_80));

    // The browser-proxied continuation endpoint exists only with the JS engine.
    #[cfg(feature = "translator-engine")]
    let router = router.route("/api/scrape/continue", post(handlers::scrape_continue));

    router
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_token,
        ))
        .layer(cors)
        .with_state(state)
}

/// Hand the shared token to a paired client.
///
/// Gated by origin in [`require_token`] rather than by the token itself — this
/// is what a client calls when it does not have one yet.
async fn pairing_token(
    axum::extract::State(state): axum::extract::State<Arc<ConnectorState>>,
) -> impl IntoResponse {
    axum::Json(serde_json::json!({ "token": state.token }))
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
    // Reap scrape sessions the extension abandoned (popup closed mid-fetch),
    // freeing their parked engine threads.
    #[cfg(feature = "translator-engine")]
    tokio::spawn(scrape_session::run_reaper(Arc::clone(
        &state.scrape_sessions,
    )));

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

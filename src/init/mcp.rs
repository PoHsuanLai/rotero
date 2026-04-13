#[cfg(feature = "desktop")]
use super::database::SHARED_DB;

#[cfg(feature = "desktop")]
pub static MCP_HTTP_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

#[cfg(feature = "desktop")]
pub(crate) fn start_mcp_server() {
    let mcp_port = 21985u16; // connector is 21984
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => {
                tracing::error!("Failed to create MCP runtime: {e}");
                return;
            }
        };
        rt.block_on(async {
            let Some((conn, lib_path)) = SHARED_DB.get() else {
                tracing::error!("MCP: SHARED_DB not initialized");
                return;
            };
            let mut mcp_db = rotero_mcp::Database::from_conn(conn.clone(), lib_path.clone());
            // Wire up change notifications so the UI refreshes after MCP writes.
            if let Some(tx) = super::connector::CONNECTOR_TX.get() {
                let tx = tx.clone();
                mcp_db.set_on_change(std::sync::Arc::new(move || {
                    let _ = tx.send(());
                }));
            }
            // Disable PDF extraction in embedded mode — pdfium can crash the HTTP server
            let pdf_available = false;
            let mcp_server = rotero_mcp::RoteroMcp::new(mcp_db, pdf_available);

            let config = rmcp::transport::StreamableHttpServerConfig::default()
                .with_stateful_mode(false)
                .with_json_response(true);

            let service = rmcp::transport::StreamableHttpService::new(
                move || Ok(mcp_server.clone()),
                std::sync::Arc::new(
                    rmcp::transport::streamable_http_server::session::local::LocalSessionManager::default(),
                ),
                config,
            );

            let app = axum::Router::new().fallback_service(service);

            let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{mcp_port}"))
                .await
            {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Failed to bind MCP port {mcp_port}: {e}");
                    return;
                }
            };
            tracing::info!("MCP server listening on 127.0.0.1:{mcp_port}");
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("MCP server error: {e}");
            }
        });
    });

    MCP_HTTP_PORT.get_or_init(|| mcp_port);
}

use super::database::SHARED_DB;

/// MCP HTTP server port — set when the embedded MCP server starts.
#[cfg(feature = "desktop")]
pub static MCP_HTTP_PORT: std::sync::OnceLock<u16> = std::sync::OnceLock::new();

/// Start the embedded MCP server over HTTP (shares DB connection with the app).
#[cfg(feature = "desktop")]
pub(crate) fn start_mcp_server() {
    let mcp_port = 21985u16; // connector is 21984
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create MCP runtime");
        rt.block_on(async {
            let Some((conn, lib_path)) = SHARED_DB.get() else {
                eprintln!("MCP: SHARED_DB not initialized");
                return;
            };
            let mcp_db = rotero_mcp::Database::from_conn(conn.clone(), lib_path.clone());
            // Disable PDF extraction in embedded mode — pdfium can crash the HTTP server.
            // The agent can still access paper metadata, annotations, and notes.
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

            let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{mcp_port}"))
                .await
                .expect("Failed to bind MCP port");
            tracing::info!("MCP server listening on 127.0.0.1:{mcp_port}");
            axum::serve(listener, app).await.unwrap();
        });
    });

    MCP_HTTP_PORT.get_or_init(|| mcp_port);
}

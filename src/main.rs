mod app;
mod db;
mod metadata;
mod state;
mod sync;
mod ui;

use std::sync::Arc;

use rotero_connector::ConnectorState;

fn main() {
    // Load config to check connector settings
    let config = sync::engine::SyncConfig::load();

    // Start the browser connector server in the background (if enabled)
    if config.connector_enabled {
        let port = config.connector_port;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                let state = Arc::new(ConnectorState {
                    on_paper_saved: None,
                    on_get_collections: None,
                });
                if let Err(e) = rotero_connector::start_server(state, port).await {
                    eprintln!("Browser connector error: {e}");
                }
            });
        });
    }

    dioxus::launch(app::App);
}

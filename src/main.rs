mod app;
mod db;
mod metadata;
mod state;
mod sync;
mod ui;

use std::sync::Arc;

use rotero_connector::ConnectorState;

fn main() {
    // Start the browser connector server in the background
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            let state = Arc::new(ConnectorState {
                on_paper_saved: None,
                on_get_collections: None,
            });
            if let Err(e) = rotero_connector::start_server(state, rotero_connector::CONNECTOR_PORT).await {
                eprintln!("Browser connector error: {e}");
            }
        });
    });

    dioxus::launch(app::App);
}

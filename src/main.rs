// Release builds on Windows are GUI apps; without this the launcher also opens
// a console window behind the WebView. Debug builds keep the console so
// `tracing` output stays visible.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod app;
mod cache;
mod init;
mod metadata;
mod state;
mod sync;
mod ui;
#[cfg(feature = "desktop")]
mod updates;

#[cfg(feature = "desktop")]
pub use init::connector::CONNECTOR_NOTIFY;
#[cfg(feature = "desktop")]
pub use init::connector::download_and_import_pdf;
#[cfg(feature = "desktop")]
pub use init::database::SHARED_DB;
#[cfg(feature = "desktop")]
pub use init::mcp::MCP_HTTP_PORT;

fn main() {
    init::logging::init_logging();

    // Reap spawned ACP agent node processes if the app is terminated by a signal
    // (Cmd+Q, Ctrl+C, `dx serve` reload) — their Drop-based cleanup won't run then.
    #[cfg(all(unix, feature = "desktop"))]
    agent::reaper::install_signal_handler();

    let config = sync::engine::SyncConfig::load();

    #[cfg(feature = "desktop")]
    {
        if let Err(e) = init::database::init_database(&config) {
            tracing::error!("Failed to initialize database: {e}");
            std::process::exit(1);
        }
        init::connector::start_connector(&config);
        init::mcp::start_mcp_server();
        init::window::launch_desktop(&config);
    }

    #[cfg(feature = "mobile")]
    {
        dioxus::LaunchBuilder::new().launch(app::App);
    }
}

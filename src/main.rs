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
        // The runtime outlives `main` deliberately. The connection opened here is
        // used later from the connector, MCP, and Dioxus runtimes; dropping the
        // runtime that created it invites a class of hang that only shows up in
        // release builds.
        let rt = Box::leak(Box::new(
            tokio::runtime::Runtime::new().expect("Failed to create init runtime"),
        ));

        match rt.block_on(init::database::init_database(&config)) {
            Ok(db) => {
                // Same invariant the tests and the bundle smoke check assert, so
                // a startup path that skips part of initialization is caught in
                // the field too — not just in CI.
                rt.block_on(init::preflight::check_database(&db));
                let _ = init::database::SHARED_DB.set(db);
                init::connector::start_connector(&config);
                init::mcp::start_mcp_server();
            }
            Err(e) => {
                // Launch anyway: the window renders the Database Error screen,
                // which is the only way a GUI user learns what went wrong. The
                // connector and MCP are skipped — with no database they would
                // only log failures of their own.
                tracing::error!("Failed to initialize database: {e}");
                init::preflight::record(|p| p.db = Some(e.clone()));
                // Read before anything else can open a database and reset it.
                init::database::DB_INIT_NEEDS_UPDATE.store(
                    rotero_db::last_open_was_newer_schema(),
                    std::sync::atomic::Ordering::Relaxed,
                );
                let _ = init::database::DB_INIT_ERROR.set(e);
            }
        }

        init::window::launch_desktop(&config);
    }

    #[cfg(feature = "mobile")]
    {
        dioxus::LaunchBuilder::new().launch(app::App);
    }
}

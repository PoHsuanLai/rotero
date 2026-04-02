mod app;
mod cache;
mod db;
mod metadata;
mod state;
mod sync;
mod ui;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rotero_connector::ConnectorState;

fn main() {
    // Initialize tracing — logs to ~/rotero-debug.log
    let log_path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("rotero-debug.log");
    let log_file = std::fs::File::create(&log_path).expect("Failed to create log file");
    let _ = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .parse("warn,rotero=debug,rotero_pdf=debug")
                .unwrap(),
        )
        .try_init();
    eprintln!("Logging to {}", log_path.display());

    // Load config to check connector settings
    let config = sync::engine::SyncConfig::load();

    // Shared flag: connector sets to true after saving, UI polls and reloads
    let connector_dirty = Arc::new(AtomicBool::new(false));

    // Start the browser connector server in the background (if enabled)
    if config.connector_enabled {
        let port = config.connector_port;
        let lib_path = config.effective_library_path();
        let dirty_flag = connector_dirty.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                // Open a dedicated DB connection for the connector thread
                let db_path = lib_path.join("rotero.db");
                let db_path_str = db_path.to_string_lossy().to_string();
                let db = match turso::Builder::new_local(&db_path_str)
                    .build()
                    .await
                {
                    Ok(db) => db,
                    Err(e) => {
                        eprintln!("Connector failed to open DB: {e}");
                        return;
                    }
                };
                let conn = match db.connect() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Connector failed to connect to DB: {e}");
                        return;
                    }
                };

                // Clone connections for each callback
                let conn_collections = conn.clone();
                let conn_tags = conn.clone();
                let conn_save = conn.clone();

                let state = Arc::new(ConnectorState {
                    on_paper_saved: Some(Box::new({
                        let dirty_flag = dirty_flag.clone();
                        move |paper, collection_id, tag_ids| {
                            let conn = conn_save.clone();
                            let dirty = dirty_flag.clone();
                            tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    match db::papers::insert_paper(&conn, &paper).await {
                                        Ok(paper_id) => {
                                            if let Some(coll_id) = collection_id {
                                                let _ = db::collections::add_paper_to_collection(&conn, paper_id, coll_id).await;
                                            }
                                            for tag_id in tag_ids {
                                                let _ = db::tags::add_tag_to_paper(&conn, paper_id, tag_id).await;
                                            }
                                            dirty.store(true, Ordering::Release);
                                            tracing::info!("Connector saved paper id={paper_id}: {}", paper.title);
                                        }
                                        Err(e) => {
                                            tracing::error!("Connector failed to save paper: {e}");
                                        }
                                    }
                                })
                            });
                        }
                    })),
                    on_get_collections: Some(Box::new(move || {
                        let conn = conn_collections.clone();
                        // Block on async in sync callback context
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                match db::collections::list_collections(&conn).await {
                                    Ok(colls) => colls
                                        .into_iter()
                                        .filter_map(|c| {
                                            Some(rotero_connector::handlers::CollectionInfo {
                                                id: c.id?,
                                                name: c.name,
                                            })
                                        })
                                        .collect(),
                                    Err(_) => Vec::new(),
                                }
                            })
                        })
                    })),
                    on_get_tags: Some(Box::new(move || {
                        let conn = conn_tags.clone();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                match db::tags::list_tags(&conn).await {
                                    Ok(tags) => tags
                                        .into_iter()
                                        .filter_map(|t| {
                                            Some(rotero_connector::handlers::TagInfo {
                                                id: t.id?,
                                                name: t.name,
                                                color: t.color,
                                            })
                                        })
                                        .collect(),
                                    Err(_) => Vec::new(),
                                }
                            })
                        })
                    })),
                });
                if let Err(e) = rotero_connector::start_server(state, port).await {
                    eprintln!("Browser connector error: {e}");
                }
            });
        });
    }

    // Store in a global so the Dioxus app can access it
    CONNECTOR_DIRTY.get_or_init(|| connector_dirty);

    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::default()
                .with_disable_context_menu(true)
        )
        .launch(app::App);
}

/// Global dirty flag set by the connector when a paper is saved via the extension.
/// The UI polls this to refresh the library.
pub static CONNECTOR_DIRTY: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

mod app;
mod cache;
mod db;
mod metadata;
mod state;
mod sync;
mod ui;

#[cfg(feature = "desktop")]
use std::sync::Arc;
#[cfg(feature = "desktop")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(feature = "desktop")]
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

    // Start the browser connector server in the background (desktop only)
    #[cfg(feature = "desktop")]
    let connector_dirty = Arc::new(AtomicBool::new(false));
    #[cfg(feature = "desktop")]
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
                        let lib_path = lib_path.clone();
                        move |paper, collection_id, tag_ids, pdf_url| {
                            let conn = conn_save.clone();
                            let dirty = dirty_flag.clone();
                            let lib_path = lib_path.clone();
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

                                            // Download PDF in background
                                            if let Some(pdf_url) = pdf_url {
                                                let conn_pdf = conn.clone();
                                                let dirty_pdf = dirty.clone();
                                                let paper_clone = paper.clone();
                                                let lib_path = lib_path.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = download_and_import_pdf(
                                                        &conn_pdf,
                                                        &lib_path,
                                                        paper_id,
                                                        &paper_clone,
                                                        &pdf_url,
                                                    )
                                                    .await
                                                    {
                                                        tracing::error!("PDF download failed for paper id={paper_id}: {e}");
                                                    } else {
                                                        dirty_pdf.store(true, Ordering::Release);
                                                    }
                                                });
                                            }

                                            // Enrich metadata in background
                                            let conn_enrich = conn.clone();
                                            let dirty_enrich = dirty.clone();
                                            tokio::spawn(async move {
                                                if let Some(enriched) = metadata::enrich::enrich_paper(&paper).await
                                                    && db::papers::update_paper_metadata(&conn_enrich, paper_id, &enriched).await.is_ok()
                                                {
                                                    dirty_enrich.store(true, Ordering::Release);
                                                    tracing::info!("Connector enriched metadata for paper id={paper_id}");
                                                }
                                            });
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
    #[cfg(feature = "desktop")]
    CONNECTOR_DIRTY.get_or_init(|| connector_dirty);

    #[cfg(feature = "desktop")]
    {
        dioxus::LaunchBuilder::new()
            .with_cfg(dioxus::desktop::Config::default().with_disable_context_menu(true))
            .launch(app::App);
    }

    #[cfg(feature = "mobile")]
    {
        dioxus::LaunchBuilder::new().launch(app::App);
    }
}

/// Global dirty flag set by the connector when a paper is saved via the extension.
/// The UI polls this to refresh the library.
#[cfg(feature = "desktop")]
pub static CONNECTOR_DIRTY: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();

/// Download a PDF from a URL and import it into the library.
#[cfg(feature = "desktop")]
async fn download_and_import_pdf(
    conn: &turso::Connection,
    lib_path: &std::path::Path,
    paper_id: i64,
    paper: &rotero_models::Paper,
    pdf_url: &str,
) -> Result<(), String> {
    tracing::info!(paper_id, pdf_url, "Downloading PDF");

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(pdf_url)
        .header("User-Agent", "Mozilla/5.0 (compatible; Rotero/0.1)")
        .send()
        .await
        .map_err(|e| format!("PDF download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("PDF download returned HTTP {}", resp.status()));
    }

    // Save to a temp file first
    let papers_dir = lib_path.join("papers");
    let tmp_dir = papers_dir.join(".tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Failed to create temp dir: {e}"))?;

    let tmp_file = tmp_dir.join(format!("download_{paper_id}.pdf"));
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read PDF bytes: {e}"))?;

    // Verify it looks like a PDF
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return Err("Downloaded file is not a valid PDF".to_string());
    }

    std::fs::write(&tmp_file, &bytes).map_err(|e| format!("Failed to write temp PDF: {e}"))?;

    // Build clean filename and import
    let first_author = paper.authors.first().map(|s| s.as_str());
    let subfolder = match paper.year {
        Some(y) => y.to_string(),
        None => "unsorted".to_string(),
    };
    let abs_dir = papers_dir.join(&subfolder);
    std::fs::create_dir_all(&abs_dir).map_err(|e| format!("Failed to create folder: {e}"))?;

    // Build filename: "Title - Author.pdf"
    let clean_title = paper
        .title
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .take(80)
        .collect::<String>()
        .trim()
        .to_string();
    let dest_name = match first_author {
        Some(a) => {
            let clean_author: String = a
                .chars()
                .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
                .take(40)
                .collect::<String>()
                .trim()
                .to_string();
            format!("{clean_title} - {clean_author}.pdf")
        }
        None => format!("{clean_title}.pdf"),
    };

    // Handle collisions
    let mut final_name = dest_name.clone();
    let mut dest = abs_dir.join(&final_name);
    let mut counter = 1;
    while dest.exists() {
        let stem = std::path::Path::new(&dest_name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        final_name = format!("{stem} ({counter}).pdf");
        dest = abs_dir.join(&final_name);
        counter += 1;
    }

    std::fs::copy(&tmp_file, &dest).map_err(|e| format!("Failed to copy PDF: {e}"))?;
    let _ = std::fs::remove_file(&tmp_file);

    let rel_path = format!("{subfolder}/{final_name}");

    // Update the paper's pdf_path in the DB
    db::papers::update_pdf_path(conn, paper_id, &rel_path)
        .await
        .map_err(|e| format!("Failed to update pdf_path: {e}"))?;

    tracing::info!(paper_id, rel_path, "PDF downloaded and imported");
    Ok(())
}

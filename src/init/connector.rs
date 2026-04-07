use std::sync::Arc;

use rotero_connector::ConnectorState;

use super::database::SHARED_DB;

/// Watch channel receiver — the connector sends a notification when data changes.
/// The UI awaits this instead of polling.
#[cfg(feature = "desktop")]
pub static CONNECTOR_NOTIFY: std::sync::OnceLock<
    std::sync::Mutex<tokio::sync::watch::Receiver<()>>,
> = std::sync::OnceLock::new();

/// Start the browser connector server in the background.
/// Returns the watch sender/receiver pair for data-change notifications.
#[cfg(feature = "desktop")]
pub(crate) fn start_connector(
    config: &crate::sync::engine::SyncConfig,
) {
    let (connector_tx, connector_rx) = tokio::sync::watch::channel(());

    if config.connector.connector_enabled {
        let port = config.connector.connector_port;
        let lib_path = config.effective_library_path();
        let connector_tx = connector_tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
            rt.block_on(async {
                // Use the shared DB connection
                let (conn, _) = match SHARED_DB.get() {
                    Some(pair) => (pair.0.clone(), pair.1.clone()),
                    None => {
                        eprintln!("Connector: SHARED_DB not initialized");
                        return;
                    }
                };

                // Clone connections for each callback
                let conn_collections = conn.clone();
                let conn_tags = conn.clone();
                let conn_save = conn.clone();

                let state = Arc::new(ConnectorState {
                    on_paper_saved: Some(Box::new({
                        let connector_tx = connector_tx.clone();
                        let lib_path = lib_path.clone();
                        move |paper, collection_id, tag_ids, pdf_url| {
                            let conn = conn_save.clone();
                            let connector_tx = connector_tx.clone();
                            let lib_path = lib_path.clone();
                            tokio::task::block_in_place(|| {
                                tokio::runtime::Handle::current().block_on(async {
                                    let mut paper = paper;
                                    if let Some(ref url) = pdf_url {
                                        paper.links.pdf_url = Some(url.clone());
                                    }
                                    match rotero_db::papers::insert_paper(&conn, &paper).await {
                                        Ok(paper_id) => {
                                            if let Some(ref coll_id) = collection_id {
                                                let _ = rotero_db::collections::add_paper_to_collection(&conn, &paper_id, coll_id).await;
                                            }
                                            for tag_id in &tag_ids {
                                                let _ = rotero_db::tags::add_tag_to_paper(&conn, &paper_id, tag_id).await;
                                            }
                                            let _ = connector_tx.send(());
                                            tracing::info!("Connector saved paper id={}: {}", paper_id, paper.title);

                                            // Download PDF in background
                                            let paper_id_enrich = paper_id.clone();
                                            if let Some(pdf_url) = pdf_url {
                                                let conn_pdf = conn.clone();
                                                let connector_tx_pdf = connector_tx.clone();
                                                let paper_clone = paper.clone();
                                                let lib_path = lib_path.clone();
                                                tokio::spawn(async move {
                                                    if let Err(e) = download_and_import_pdf(
                                                        &conn_pdf,
                                                        &lib_path,
                                                        &paper_id,
                                                        &paper_clone,
                                                        &pdf_url,
                                                    )
                                                    .await
                                                    {
                                                        tracing::error!("PDF download failed for paper id={}: {e}", paper_id);
                                                    } else {
                                                        let _ = connector_tx_pdf.send(());
                                                    }
                                                });
                                            }

                                            // Enrich metadata in background
                                            let conn_enrich = conn.clone();
                                            let connector_tx_enrich = connector_tx.clone();
                                            tokio::spawn(async move {
                                                if let Some(enriched) = crate::metadata::enrich::enrich_paper(&paper).await
                                                    && rotero_db::papers::update_paper_metadata(&conn_enrich, &paper_id_enrich, &enriched).await.is_ok()
                                                {
                                                    let _ = connector_tx_enrich.send(());
                                                    tracing::info!("Connector enriched metadata for paper id={}", paper_id_enrich);
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
                                match rotero_db::collections::list_collections(&conn).await {
                                    Ok(colls) => colls
                                        .into_iter()
                                        .filter_map(|c| {
                                            Some(rotero_connector::handlers::CollectionInfo {
                                                id: c.id.clone()?,
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
                                match rotero_db::tags::list_tags(&conn).await {
                                    Ok(tags) => tags
                                        .into_iter()
                                        .filter_map(|t| {
                                            Some(rotero_connector::handlers::TagInfo {
                                                id: t.id.clone()?,
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
                    translation_server: tokio::sync::RwLock::new(None),
                });

                // Start translation server in background (don't block connector)
                {
                    let state_clone = state.clone();
                    tokio::spawn(async move {
                        let ts = rotero_translate::TranslationServer::new(1969);
                        match ts.ensure_running().await {
                            Ok(()) => {
                                tracing::info!("Zotero translation server started");
                                *state_clone.translation_server.write().await = Some(ts);
                            }
                            Err(e) => {
                                tracing::warn!("Failed to start translation server: {e}");
                            }
                        }
                    });
                }

                if let Err(e) = rotero_connector::start_server(state, port).await {
                    eprintln!("Browser connector error: {e}");
                }
            });
        });
    }

    // Store the watch receiver so the Dioxus app can await notifications
    CONNECTOR_NOTIFY.get_or_init(|| std::sync::Mutex::new(connector_rx));
}

/// Download a PDF from a URL and import it into the library.
pub async fn download_and_import_pdf(
    conn: &rotero_db::turso::Connection,
    lib_path: &std::path::Path,
    paper_id: &str,
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
    rotero_db::papers::update_pdf_path(conn, paper_id, &rel_path)
        .await
        .map_err(|e| format!("Failed to update pdf_path: {e}"))?;

    tracing::info!(
        paper_id = paper_id,
        rel_path = rel_path.as_str(),
        "PDF downloaded and imported"
    );
    Ok(())
}

#[cfg(feature = "desktop")]
use std::sync::Arc;

#[cfg(feature = "desktop")]
use rotero_connector::ConnectorState;

#[cfg(feature = "desktop")]
use super::database::SHARED_DB;

#[cfg(feature = "desktop")]
pub static CONNECTOR_NOTIFY: std::sync::OnceLock<
    std::sync::Mutex<tokio::sync::watch::Receiver<()>>,
> = std::sync::OnceLock::new();

#[cfg(feature = "desktop")]
pub(crate) fn start_connector(config: &crate::sync::engine::SyncConfig) {
    let (connector_tx, connector_rx) = tokio::sync::watch::channel(());

    if config.connector.connector_enabled {
        let port = config.connector.connector_port;
        let lib_path = config.effective_library_path();
        let connector_tx = connector_tx.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("Failed to create connector runtime: {e}");
                    return;
                }
            };
            rt.block_on(async {
                let (conn, _) = match SHARED_DB.get() {
                    Some(pair) => (pair.0.clone(), pair.1.clone()),
                    None => {
                        tracing::error!("Connector: SHARED_DB not initialized");
                        return;
                    }
                };

                let conn_collections = conn.clone();
                let conn_tags = conn.clone();
                let conn_save = conn.clone();
                let conn_search = conn.clone();
                let conn_get_by_ids = conn.clone();

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
                    on_search_papers: Some(Box::new(move |query: &str| {
                        let conn = conn_search.clone();
                        let query = query.to_string();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                rotero_db::papers::search_papers(&conn, &query)
                                    .await
                                    .unwrap_or_default()
                            })
                        })
                    })),
                    on_get_papers_by_ids: Some(Box::new(move |ids: &[String]| {
                        let conn = conn_get_by_ids.clone();
                        let ids = ids.to_vec();
                        tokio::task::block_in_place(|| {
                            tokio::runtime::Handle::current().block_on(async {
                                rotero_db::papers::get_papers_by_ids(&conn, &ids)
                                    .await
                                    .unwrap_or_default()
                            })
                        })
                    })),
                    translation_server: tokio::sync::RwLock::new(None),
                });

                {
                    let state_clone = state.clone();
                    tokio::spawn(async move {
                        let result: Result<_, String> = (|| {
                            let node_bin = crate::agent::node::find_or_install_node()?;
                            let npm_bin = crate::agent::node::find_npm()?;
                            Ok((node_bin, npm_bin))
                        })();
                        let (node_bin, npm_bin) = match result {
                            Ok(bins) => bins,
                            Err(e) => {
                                tracing::warn!("Cannot start translation server: {e}");
                                return;
                            }
                        };
                        let ts = rotero_translate::TranslationServer::new(1969, node_bin, npm_bin);
                        match ts.ensure_running().await {
                            Ok(()) => {
                                tracing::info!("Zotero translation server started");
                                *state_clone.translation_server.write().await = Some(ts);
                            }
                            Err(e) => tracing::warn!("Failed to start translation server: {e}"),
                        }
                    });
                }

                if let Err(e) = rotero_connector::start_server(state, port).await {
                    tracing::error!("Browser connector error: {e}");
                }
            });
        });
    }

    CONNECTOR_NOTIFY.get_or_init(|| std::sync::Mutex::new(connector_rx));
}

pub async fn download_and_import_pdf(
    conn: &rotero_db::turso::Connection,
    lib_path: &std::path::Path,
    paper_id: &str,
    paper: &rotero_models::Paper,
    pdf_url: &str,
) -> Result<(), String> {
    tracing::info!(paper_id, pdf_url, "Downloading PDF");

    let db = rotero_db::Database::from_conn(conn.clone(), lib_path.to_path_buf());
    let first_author = paper.authors.first().map(|s| s.as_str());

    let rel_path = crate::metadata::pdf_download::download_and_save_pdf(
        &db,
        &[pdf_url.to_string()],
        &paper.title,
        first_author,
        paper.year,
    )
    .await
    .map_err(|e| format!("{e}"))?;

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

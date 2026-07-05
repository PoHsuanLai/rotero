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
pub static CONNECTOR_TX: std::sync::OnceLock<tokio::sync::watch::Sender<()>> =
    std::sync::OnceLock::new();

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
                let (conn, db_lib_path) = match SHARED_DB.get() {
                    Some(pair) => (pair.0.clone(), pair.1.clone()),
                    None => {
                        tracing::error!("Connector: SHARED_DB not initialized");
                        return;
                    }
                };
                let db = rotero_db::Database::from_conn(conn.clone(), db_lib_path);

                let db_collections = db.clone();
                let db_tags = db.clone();
                let db_save = db.clone();
                let db_search = db.clone();
                let db_get_by_ids = db.clone();

                let state = Arc::new(ConnectorState {
                    on_paper_saved: Some(Box::new({
                        let connector_tx = connector_tx.clone();
                        let lib_path = lib_path.clone();
                        move |paper, collection_id, tag_ids, pdf_url| {
                            let db = db_save.clone();
                            let connector_tx = connector_tx.clone();
                            let lib_path = lib_path.clone();
                            Box::pin(async move {
                                let mut paper = paper;
                                if let Some(ref url) = pdf_url {
                                    paper.links.pdf_url = Some(url.clone());
                                }
                                match db.insert_paper(&paper).await {
                                    Ok(paper_id) => {
                                        if let Some(ref coll_id) = collection_id {
                                            let _ = db
                                                .add_paper_to_collection(&paper_id, coll_id)
                                                .await;
                                        }
                                        for tag_id in &tag_ids {
                                            let _ = db.add_tag_to_paper(&paper_id, tag_id).await;
                                        }
                                        let _ = connector_tx.send(());
                                        tracing::info!(
                                            "Connector saved paper id={}: {}",
                                            paper_id,
                                            paper.title
                                        );

                                        let paper_id_enrich = paper_id.clone();
                                        if let Some(pdf_url) = pdf_url {
                                            let db_pdf = db.clone();
                                            let connector_tx_pdf = connector_tx.clone();
                                            let paper_clone = paper.clone();
                                            let lib_path = lib_path.clone();
                                            tokio::spawn(async move {
                                                if let Err(e) = download_and_import_pdf(
                                                    &db_pdf,
                                                    &lib_path,
                                                    &paper_id,
                                                    &paper_clone,
                                                    &pdf_url,
                                                )
                                                .await
                                                {
                                                    tracing::error!(
                                                        "PDF download failed for paper id={}: {e}",
                                                        paper_id
                                                    );
                                                } else {
                                                    let _ = connector_tx_pdf.send(());
                                                }
                                            });
                                        }

                                        let db_enrich = db.clone();
                                        let connector_tx_enrich = connector_tx.clone();
                                        tokio::spawn(async move {
                                            if let Some(enriched) =
                                                crate::metadata::enrich::enrich_paper(&paper).await
                                                && db_enrich
                                                    .update_paper_metadata(
                                                        &paper_id_enrich,
                                                        &enriched,
                                                    )
                                                    .await
                                                    .is_ok()
                                            {
                                                let _ = connector_tx_enrich.send(());
                                                tracing::info!(
                                                    "Connector enriched metadata for paper id={}",
                                                    paper_id_enrich
                                                );
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        tracing::error!("Connector failed to save paper: {e}");
                                    }
                                }
                            })
                        }
                    })),
                    on_get_collections: Some(Box::new(move || {
                        let db = db_collections.clone();
                        Box::pin(async move {
                            match db.list_collections().await {
                                Ok(colls) => colls
                                    .into_iter()
                                    .filter_map(|c| {
                                        Some(rotero_connector::handlers::CollectionInfo {
                                            id: c.id.clone()?,
                                            name: c.name,
                                            parent_id: c.parent_id,
                                        })
                                    })
                                    .collect(),
                                Err(_) => Vec::new(),
                            }
                        })
                    })),
                    on_get_tags: Some(Box::new(move || {
                        let db = db_tags.clone();
                        Box::pin(async move {
                            match db.list_tags().await {
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
                    })),
                    on_search_papers: Some(Box::new(move |query: String| {
                        let db = db_search.clone();
                        Box::pin(async move { db.search_papers(&query).await.unwrap_or_default() })
                    })),
                    on_get_papers_by_ids: Some(Box::new(move |ids: Vec<String>| {
                        let db = db_get_by_ids.clone();
                        Box::pin(
                            async move { db.get_papers_by_ids(&ids).await.unwrap_or_default() },
                        )
                    })),
                    translator_registry: rotero_translate::TranslatorRegistry::with_builtins(),
                });

                if let Err(e) = rotero_connector::start_server(state, port).await {
                    tracing::error!("Browser connector error: {e}");
                }
            });
        });
    }

    CONNECTOR_TX.get_or_init(|| connector_tx);
    CONNECTOR_NOTIFY.get_or_init(|| std::sync::Mutex::new(connector_rx));
}

pub async fn download_and_import_pdf(
    db: &rotero_db::Database,
    _lib_path: &std::path::Path,
    paper_id: &str,
    paper: &rotero_models::Paper,
    pdf_url: &str,
) -> Result<(), String> {
    tracing::info!(paper_id, pdf_url, "Downloading PDF");

    let first_author = paper.authors.first().map(|s| s.as_str());

    let rel_path = crate::metadata::pdf_download::download_and_save_pdf(
        db,
        &[pdf_url.to_string()],
        &paper.title,
        first_author,
        paper.year,
    )
    .await
    .map_err(|e| format!("{e}"))?;

    db.update_pdf_path(paper_id, &rel_path)
        .await
        .map_err(|e| format!("Failed to update pdf_path: {e}"))?;

    tracing::info!(
        paper_id = paper_id,
        rel_path = rel_path.as_str(),
        "PDF downloaded and imported"
    );
    Ok(())
}

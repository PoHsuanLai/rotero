use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

/// Loads library data from DB into signals on startup and when DB reloads.
#[component]
pub fn LoadLibraryData() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

    // Initial load
    let db2 = db.clone();
    use_effect(move || {
        let db = db2.clone();
        spawn(async move {
            // Check for external modifications (e.g. synced from another device)
            let db_path = db.data_dir().join("rotero.db");
            if crate::sync::engine::check_external_modification(&db_path, None) {
                eprintln!("Database was modified externally, reloading...");
            }

            let conn = db.conn();
            if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                lib_state.with_mut(|s| s.papers = papers);
            }
            if let Ok(collections) = rotero_db::collections::list_collections(conn).await {
                lib_state.with_mut(|s| s.collections = collections);
            }
            if let Ok(tags) = rotero_db::tags::list_tags(conn).await {
                lib_state.with_mut(|s| s.tags = tags);
            }
            if let Ok(searches) = rotero_db::saved_searches::list_saved_searches(conn).await {
                lib_state.with_mut(|s| s.saved_searches = searches);
            }

            // Store current modification time for future checks
            let _ = crate::sync::engine::file_modified_time(&db_path);
        });
    });

    // Background citation count refresh
    #[cfg(feature = "desktop")]
    {
        let db_cite = db.clone();
        use_future(move || {
            let db = db_cite.clone();
            async move {
                // Wait for initial library load to complete
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                loop {
                    // Find papers with DOI but no citation count — query DB directly
                    // to avoid cloning the entire papers Vec.
                    let needs_update = rotero_db::papers::list_papers_needing_citations(db.conn())
                        .await
                        .unwrap_or_default();

                    for (paper_id, doi) in needs_update {
                        let result = if doi.starts_with("arXiv:") {
                            let arxiv_id = doi.strip_prefix("arXiv:").unwrap_or(&doi);
                            crate::metadata::semantic_scholar::fetch_by_arxiv_id(arxiv_id).await
                        } else {
                            crate::metadata::semantic_scholar::fetch_by_doi(&doi).await
                        };

                        match result {
                            Ok(meta) => {
                                if let Some(count) = meta.citation.citation_count {
                                    let _ = rotero_db::papers::update_citation_count(
                                        db.conn(),
                                        &paper_id,
                                        count,
                                    )
                                    .await;
                                    lib_state.with_mut(|s| {
                                        if let Some(p) = s.papers.iter_mut().find(|p| {
                                            p.id.as_deref() == Some(paper_id.as_str())
                                        }) {
                                            p.citation.citation_count = Some(count);
                                        }
                                    });
                                }
                                // Normal rate limit: 3 seconds between requests
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            }
                            Err(e) => {
                                if e.contains("429") {
                                    // Rate limited — back off for 60 seconds
                                    tracing::debug!("S2 rate limited, backing off 60s");
                                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                                } else {
                                    tracing::debug!("Citation count fetch failed for {doi}: {e}");
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                }
                            }
                        }
                    }

                    // Re-check every hour
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            }
        });
    }

    // Background citation key generation + auto-export
    #[cfg(feature = "desktop")]
    {
        let db_bib = db.clone();
        use_future(move || {
            let db = db_bib.clone();
            async move {
                // Wait for initial load
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;

                loop {
                    // Generate citation keys for papers that don't have one
                    let existing_keys = rotero_db::papers::list_citation_keys(db.conn())
                        .await
                        .unwrap_or_default();

                    // Query DB directly for papers needing keys — avoids cloning
                    // the entire papers Vec every 30 seconds.
                    let needs_keys =
                        rotero_db::papers::list_papers_needing_citation_keys(db.conn())
                            .await
                            .unwrap_or_default();
                    let mut keys_updated = false;
                    let mut all_keys = existing_keys;

                    for (paper_id, title, authors, year) in &needs_keys {
                        // Build a minimal Paper for key generation (only needs authors + year)
                        let stub = rotero_models::Paper {
                            id: Some(paper_id.clone()),
                            title: title.clone(),
                            authors: authors.clone(),
                            year: *year,
                            ..Default::default()
                        };

                        let key = rotero_bib::generate_unique_cite_key(&stub, &all_keys);
                        if rotero_db::papers::update_citation_key(db.conn(), paper_id, &key)
                            .await
                            .is_ok()
                        {
                            let pid = paper_id.clone();
                            lib_state.with_mut(|s| {
                                if let Some(p) = s.papers.iter_mut().find(|p| {
                                    p.id.as_deref() == Some(pid.as_str())
                                }) {
                                    p.citation.citation_key = Some(key.clone());
                                }
                            });
                            all_keys.push(key);
                            keys_updated = true;
                        }
                    }

                    // Auto-export .bib if configured and keys were updated
                    if keys_updated {
                        let config = config.read();
                        if let Some(ref bib_path) = config.sync.auto_export_bib_path {
                            let state = lib_state.read();
                            let bib_content = rotero_bib::export_bibtex(&state.papers);
                            if let Err(e) = std::fs::write(bib_path, &bib_content) {
                                tracing::warn!("Auto-export .bib failed: {e}");
                            }
                        }
                    }

                    // Re-check every 30 seconds (quick for new imports)
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            }
        });
    }

    // Await connector notifications to refresh after browser extension saves
    #[cfg(feature = "desktop")]
    use_future(move || {
        let db = db.clone();
        async move {
            use crate::state::app_state::LibraryView;
            // Clone the receiver out of the mutex so we can await it without holding the lock
            let mut rx = {
                let Some(lock) = crate::CONNECTOR_NOTIFY.get() else {
                    return;
                };
                let guard = lock.lock().unwrap();
                guard.clone()
            };
            loop {
                // Blocks until the connector actually sends a notification — zero CPU when idle
                if rx.changed().await.is_err() {
                    break; // sender dropped
                }
                let conn = db.conn();
                if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                    lib_state.with_mut(|s| s.papers = papers);
                }
                let view = lib_state.read().view.clone();
                match view {
                    LibraryView::Collection(coll_id) => {
                        if let Ok(ids) =
                            rotero_db::collections::list_paper_ids_in_collection(conn, &coll_id)
                                .await
                        {
                            lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                        }
                    }
                    LibraryView::Tag(tag_id) => {
                        if let Ok(ids) = rotero_db::tags::list_paper_ids_by_tag(conn, &tag_id).await
                        {
                            lib_state.with_mut(|s| s.filter.tag_paper_ids = Some(ids));
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    rsx! {}
}

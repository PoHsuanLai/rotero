use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

#[component]
pub fn LoadLibraryData() -> Element {
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let db = use_context::<Database>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    #[cfg(feature = "desktop")]
    let render_ch = use_context::<crate::app::RenderChannel>();

    let db2 = db.clone();
    use_effect(move || {
        let db = db2.clone();
        spawn(async move {
            let db_path = db.data_dir().join("rotero.db");
            if crate::sync::engine::check_external_modification(&db_path, None) {
                tracing::info!("Database was modified externally, reloading...");
            }

            crate::state::commands::refresh_papers(&db, &mut lib_state).await;
            if let Ok(collections) = db.list_collections().await {
                lib_state.with_mut(|s| s.collections = collections);
            }
            if let Ok(tags) = db.list_tags().await {
                lib_state.with_mut(|s| s.tags = tags);
            }
            if let Ok(searches) = db.list_saved_searches().await {
                lib_state.with_mut(|s| s.saved_searches = searches);
            }

            let _ = crate::sync::engine::file_modified_time(&db_path);
        });
    });

    #[cfg(feature = "desktop")]
    {
        let db_cite = db.clone();
        use_future(move || {
            let db = db_cite.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;

                loop {
                    let needs_update = db.list_papers_needing_citations().await.unwrap_or_default();

                    for (paper_id, doi) in needs_update {
                        let result = crate::metadata::semantic_scholar::fetch_by_doi(&doi).await;

                        match result {
                            Ok(meta) => {
                                if let Some(count) = meta.citation.citation_count {
                                    let _ = db.update_citation_count(&paper_id, count).await;
                                    lib_state.with_mut(|s| {
                                        if let Some(p) = s
                                            .papers
                                            .iter_mut()
                                            .find(|p| p.id.as_deref() == Some(paper_id.as_str()))
                                        {
                                            p.citation.citation_count = Some(count);
                                        }
                                    });
                                }
                                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                            }
                            Err(e) => {
                                if e.contains("429") {
                                    tracing::debug!("S2 rate limited, backing off 60s");
                                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                                } else {
                                    tracing::debug!("Citation count fetch failed for {doi}: {e}");
                                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                                }
                            }
                        }
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                }
            }
        });
    }

    // One-time citation-graph population. Extracts each PDF's links, resolves them
    // to library papers, and records directed citation edges. Guarded by an
    // app_flags row so it runs once per install.
    #[cfg(feature = "desktop")]
    {
        let db_cites = db.clone();
        use_future(move || {
            let db = db_cites.clone();
            let render_tx = render_ch.sender();
            async move {
                // Let the initial UI + render thread settle first.
                tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                crate::state::commands::scan_citations_if_needed(&render_tx, &db).await;
            }
        });
    }

    #[cfg(feature = "desktop")]
    {
        let db_bib = db.clone();
        use_future(move || {
            let db = db_bib.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_secs(4)).await;

                loop {
                    let existing_keys = db.list_citation_keys().await.unwrap_or_default();

                    let needs_keys = db
                        .list_papers_needing_citation_keys()
                        .await
                        .unwrap_or_default();
                    let mut keys_updated = false;
                    let mut all_keys = existing_keys;

                    for (paper_id, title, authors, year) in &needs_keys {
                        let stub = rotero_models::Paper {
                            id: Some(paper_id.clone()),
                            title: title.clone(),
                            creators: authors
                                .iter()
                                .map(|a| rotero_models::Creator::author_from_display(a))
                                .collect(),
                            year: *year,
                            ..Default::default()
                        };

                        let key = rotero_bib::generate_unique_cite_key(&stub, &all_keys);
                        if db.update_citation_key(paper_id, &key).await.is_ok() {
                            let pid = paper_id.clone();
                            lib_state.with_mut(|s| {
                                if let Some(p) = s
                                    .papers
                                    .iter_mut()
                                    .find(|p| p.id.as_deref() == Some(pid.as_str()))
                                {
                                    p.citation.citation_key = Some(key.clone());
                                }
                            });
                            all_keys.push(key);
                            keys_updated = true;
                        }
                    }

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

                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
            }
        });
    }

    // Backfill content hashes for PDFs imported before `pdf_sha256` existed.
    //
    // Sync addresses a shared PDF by its hash, so a row without one is simply
    // skipped by the transfer — its file never publishes and never arrives.
    // This drains the queue in small batches rather than hashing the whole
    // library at once: a library of thousands of PDFs must stay responsive
    // while it catches up, and reading every one of them is the single most
    // expensive thing startup could do.
    //
    // Resumable by construction — the queue is "has a path, has no hash", so an
    // interrupted run simply picks up where it stopped, and a completed one
    // costs one empty query per pass.
    #[cfg(feature = "desktop")]
    {
        let db_hash = db.clone();
        use_future(move || {
            let db = db_hash.clone();
            async move {
                // Let the library finish loading first; this is repair work,
                // not anything the user is waiting on.
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;

                loop {
                    let pending = db
                        .list_papers_needing_pdf_hashes()
                        .await
                        .unwrap_or_default();
                    if pending.is_empty() {
                        break;
                    }

                    // Whether this pass hashed anything. A row whose file is
                    // missing stays in the queue, so without this the loop
                    // would re-read the same unreadable rows every 5 seconds
                    // for as long as the app runs.
                    let mut progressed = false;

                    for (paper_id, rel_path) in pending.iter().take(20) {
                        let hasher = db.clone();
                        let (id, path) = (paper_id.clone(), rel_path.clone());
                        // Hashing reads the whole file, so keep it off the UI
                        // thread the way the PDF import path already does.
                        let hashed = tokio::task::spawn_blocking(move || {
                            hasher.hash_stored_pdf(&path).map(|h| (id, h))
                        })
                        .await;

                        match hashed {
                            Ok(Ok((id, hash))) => match db.set_pdf_sha256(&id, &hash).await {
                                Ok(()) => progressed = true,
                                Err(e) => tracing::warn!("Could not record PDF hash: {e}"),
                            },
                            // A paper whose file is gone — deleted outside the
                            // app, or never downloaded. Nothing to hash, and
                            // retrying will not change that.
                            Ok(Err(e)) => tracing::debug!("Skipping PDF hash: {e}"),
                            Err(e) => tracing::warn!("PDF hash task failed: {e}"),
                        }
                    }

                    if !progressed {
                        tracing::debug!(
                            "PDF hash backfill stopping: {} row(s) name files that \
                             cannot be read",
                            pending.len()
                        );
                        break;
                    }

                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }

                tracing::info!("PDF hash backfill complete");
            }
        });
    }

    #[cfg(feature = "desktop")]
    use_future(move || {
        let db = db.clone();
        async move {
            use crate::state::app_state::LibraryView;
            let mut rx = {
                let Some(lock) = crate::CONNECTOR_NOTIFY.get() else {
                    return;
                };
                let guard = lock.lock().unwrap();
                guard.clone()
            };
            loop {
                if rx.changed().await.is_err() {
                    break;
                }
                crate::state::commands::refresh_papers(&db, &mut lib_state).await;
                let view = lib_state.read().view.clone();
                match view {
                    LibraryView::Collection(coll_id) => {
                        if let Ok(ids) = db.list_paper_ids_in_subtree(&coll_id).await {
                            lib_state.with_mut(|s| s.filter.collection_paper_ids = Some(ids));
                        }
                    }
                    LibraryView::Tag(tag_id) => {
                        if let Ok(ids) = db.list_paper_ids_by_tag(&tag_id).await {
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

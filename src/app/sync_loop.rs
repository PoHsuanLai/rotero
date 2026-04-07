use dioxus::prelude::*;

use crate::state::app_state::LibraryState;
use rotero_db::Database;

#[component]
pub fn SyncLoop() -> Element {
    let db = use_context::<Database>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();

    use_future(move || {
        let db = db.clone();
        async move {
            #[cfg(feature = "cloudkit")]
            let mut ck_engine: Option<crate::sync::cloudkit_sync::CloudKitSyncEngine> = None;

            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;

                let cfg = config.read().clone();
                if !cfg.sync.sync_enabled {
                    continue;
                }

                let conn = db.conn();
                let site_id = match rotero_db::crr::site_id(conn).await {
                    Ok(id) => id,
                    Err(_) => continue,
                };

                let applied = match cfg.sync.sync_transport {
                    crate::sync::engine::SyncTransport::File => {
                        let Some(ref folder) = cfg.sync.sync_folder_path else {
                            continue;
                        };
                        let engine = crate::sync::file_sync::FileSyncEngine::new(
                            std::path::PathBuf::from(folder),
                            site_id,
                        );
                        if let Err(e) = engine.export_changes(conn).await {
                            tracing::warn!("File sync export failed: {e}");
                        }
                        let imported = match engine.import_changes(conn).await {
                            Ok(n) => n,
                            Err(e) => {
                                tracing::warn!("File sync import failed: {e}");
                                0
                            }
                        };
                        let papers_dir = db.papers_dir();
                        let papers = lib_state.read().papers.clone();
                        for paper in &papers {
                            if let Some(ref path) = paper.links.pdf_path {
                                let _ = engine.export_pdf(&papers_dir, path);
                                let _ = engine.import_pdf(&papers_dir, path);
                            }
                        }
                        imported
                    }
                    crate::sync::engine::SyncTransport::CloudKit => {
                        #[cfg(feature = "cloudkit")]
                        {
                            let engine = ck_engine.get_or_insert_with(|| {
                                crate::sync::cloudkit_sync::CloudKitSyncEngine::new(site_id.clone())
                                    .expect("Failed to init CloudKit")
                            });
                            if let Err(e) = engine.export_changes(conn).await {
                                tracing::warn!("CloudKit export failed: {e}");
                            }
                            match engine.import_changes(conn).await {
                                Ok(n) => n,
                                Err(e) => {
                                    tracing::warn!("CloudKit import failed: {e}");
                                    0
                                }
                            }
                        }
                        #[cfg(not(feature = "cloudkit"))]
                        {
                            tracing::warn!("CloudKit sync selected but not compiled in");
                            0
                        }
                    }
                };

                if applied > 0 {
                    tracing::info!("Sync imported {applied} changes, refreshing library");
                    if let Ok(papers) = rotero_db::papers::list_papers(conn).await {
                        lib_state.with_mut(|s| s.papers = papers);
                    }
                    if let Ok(collections) = rotero_db::collections::list_collections(conn).await {
                        lib_state.with_mut(|s| s.collections = collections);
                    }
                    if let Ok(tags) = rotero_db::tags::list_tags(conn).await {
                        lib_state.with_mut(|s| s.tags = tags);
                    }
                }
            }
        }
    });

    rsx! {}
}

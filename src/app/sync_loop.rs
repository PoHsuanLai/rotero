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

                let site_id = match db.crr().site_id().await {
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
                        // A sync that has been failing for days looks identical
                        // to one that has nothing to do, so record the reason
                        // where the user can see it and clear it once a round
                        // trip succeeds.
                        let mut failure: Option<String> = None;
                        if let Err(e) = engine.export_changes(&db).await {
                            tracing::warn!("File sync export failed: {e}");
                            failure = Some(format!("could not send changes: {e}"));
                        }
                        let imported = match engine.import_changes(&db).await {
                            Ok(n) => n,
                            Err(e) => {
                                tracing::warn!("File sync import failed: {e}");
                                failure = Some(format!("could not receive changes: {e}"));
                                0
                            }
                        };
                        crate::init::preflight::record(|p| p.sync_folder = failure);
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
                            // Reported, not panicked: this runs inside a UI
                            // future, so an unavailable iCloud account used to
                            // take the whole app down.
                            if ck_engine.is_none() {
                                match crate::sync::cloudkit_sync::CloudKitSyncEngine::new(
                                    site_id.clone(),
                                ) {
                                    Ok(engine) => ck_engine = Some(engine),
                                    Err(e) => {
                                        tracing::error!("CloudKit unavailable: {e}");
                                        crate::init::preflight::record(|p| {
                                            p.sync_folder =
                                                Some(format!("iCloud sync is unavailable: {e}"));
                                        });
                                        // Skip this tick and retry on the next,
                                        // as the file transport does when its
                                        // folder is unset.
                                        continue;
                                    }
                                }
                            }
                            let engine = ck_engine.as_mut().expect("just initialized");
                            if let Err(e) = engine.export_changes(&db).await {
                                tracing::warn!("CloudKit export failed: {e}");
                            }
                            match engine.import_changes(&db).await {
                                Ok(n) => n,
                                Err(e) => {
                                    tracing::warn!("CloudKit import failed: {e}");
                                    0
                                }
                            }
                        }
                        #[cfg(not(feature = "cloudkit"))]
                        {
                            // The settings pane renders the file-sync UI in this
                            // state, so sync looks configured and simply never
                            // runs. Say so rather than logging into the void.
                            tracing::warn!("CloudKit sync selected but not compiled in");
                            crate::init::preflight::record(|p| {
                                p.sync_folder = Some(
                                    "iCloud sync is selected but this build does not include it; \
                                     choose folder sync in Settings"
                                        .to_string(),
                                );
                            });
                            0
                        }
                    }
                };

                if applied > 0 {
                    tracing::info!("Sync imported {applied} changes, refreshing library");
                    crate::state::commands::refresh_papers(&db, &mut lib_state).await;
                    if let Ok(collections) = db.list_collections().await {
                        lib_state.with_mut(|s| s.collections = collections);
                    }
                    if let Ok(tags) = db.list_tags().await {
                        lib_state.with_mut(|s| s.tags = tags);
                    }
                }
            }
        }
    });

    rsx! {}
}

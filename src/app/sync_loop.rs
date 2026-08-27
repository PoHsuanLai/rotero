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

                // Read from the handle rather than the change-tracking store:
                // the identity is Rotero's own, held since the database opened,
                // and it must outlive that dependency.
                let Some(site_id) = hex_bytes(db.device_id()) else {
                    continue;
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
                        // Reported jointly with the PDF half at the end of the
                        // tick — see below. Publishing here as well would let
                        // whichever ran last decide what the user sees, so a
                        // snapshot failure could be erased by PDFs succeeding.

                        // Drop tombstones every peer has certainly seen. Rate-
                        // limited internally to roughly weekly, and a no-op
                        // unless every peer's snapshot is well past them, so
                        // running it on the sync tick costs a flag read.
                        //
                        // Collect the orphaned PDF hashes *before* reaping:
                        // reaping removes the tombstones that carry them, after
                        // which nothing records which shared blobs the deletion
                        // left behind.
                        let orphaned = db.orphaned_pdf_hashes().await.unwrap_or_else(|e| {
                            tracing::warn!("Could not list orphaned PDF hashes: {e}");
                            Vec::new()
                        });

                        match db
                            .reap_tombstones(
                                engine.peer_horizon().await,
                                chrono::Utc::now().timestamp_millis(),
                            )
                            .await
                        {
                            Ok(stats) if stats.removed > 0 => {
                                tracing::info!("Reaped {} settled tombstone(s)", stats.removed);

                                // Only now, having reaped, is the deletion past
                                // every peer's horizon and the TTL — so the
                                // shared copy is safe to drop. The device's own
                                // file is deliberately left alone: it is the
                                // user's data, and sync does not unlink it.
                                for sha256 in &orphaned {
                                    if let Err(e) = engine.reap_shared_pdf(sha256) {
                                        tracing::warn!("Could not reap shared PDF: {e}");
                                    }
                                }
                            }
                            Ok(_) => {}
                            Err(e) => tracing::warn!("Tombstone reap failed: {e}"),
                        }

                        // Drive PDF transfer from the database, not from the
                        // UI's paper list. `lib_state.papers` is
                        // `list_papers()`, capped at the 500 most recently
                        // added — so every library past that cap had the rest
                        // of its PDFs silently excluded from sync in both
                        // directions. Reading it here also meant working from
                        // the pre-merge snapshot, since the refresh below runs
                        // after this block.
                        //
                        // Blocking file copies, moved off the UI thread: this
                        // runs inside a Dioxus future, and multi-megabyte reads
                        // and writes over cloud-synced storage stalled it.
                        let papers_dir = db.papers_dir();
                        let pdf_failure = match db.list_papers_with_pdfs().await {
                            Ok(papers) => {
                                let engine = engine.clone();
                                tokio::task::spawn_blocking(move || {
                                    let mut first_error: Option<String> = None;
                                    for (_, path, sha256) in &papers {
                                        for outcome in [
                                            engine.export_pdf(&papers_dir, path, sha256.as_deref()),
                                            engine.import_pdf(&papers_dir, path, sha256.as_deref()),
                                        ] {
                                            if let Err(e) = outcome {
                                                tracing::warn!("PDF sync failed for {path}: {e}");
                                                first_error.get_or_insert(e);
                                            }
                                        }
                                    }
                                    first_error
                                })
                                .await
                                .unwrap_or_else(|e| Some(format!("PDF sync task failed: {e}")))
                            }
                            Err(e) => {
                                tracing::warn!("Could not list papers with PDFs: {e}");
                                Some(format!("could not list PDFs: {e}"))
                            }
                        };

                        // Report both halves together, rather than discarding
                        // the PDF result the way `let _ =` used to. A sync that
                        // has been failing for days otherwise looks exactly
                        // like one with nothing to do.
                        //
                        // One field holds both, so a failure in either half has
                        // to survive the other succeeding: the snapshot error
                        // leads when there is one, since a library that is not
                        // exchanging rows is the larger problem.
                        let combined = match (failure, pdf_failure) {
                            (Some(snapshot), _) => Some(snapshot),
                            (None, Some(pdf)) => Some(format!("could not sync PDFs: {pdf}")),
                            (None, None) => None,
                        };
                        crate::init::preflight::record(|p| p.sync_folder = combined);

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

/// Decode a lowercase-hex device id into the bytes the sync engines name files
/// with.
fn hex_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || hex.is_empty() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::hex_bytes;

    /// The decoded id must match what the engines write into filenames.
    ///
    /// A device whose id changed shape would look like a brand-new peer and
    /// re-send its whole library, so this round-trip is load-bearing.
    #[test]
    fn a_device_id_round_trips_through_hex() {
        let bytes: Vec<u8> = (0u8..16).collect();
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex_bytes(&hex), Some(bytes));
    }

    #[test]
    fn a_malformed_device_id_is_rejected() {
        assert_eq!(hex_bytes(""), None, "empty");
        assert_eq!(hex_bytes("abc"), None, "odd length");
        assert_eq!(hex_bytes("zz"), None, "not hex");
    }
}

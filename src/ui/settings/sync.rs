use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;

#[component]
pub fn SyncSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut status_msg = use_signal(|| None::<String>);

    let enabled = config.read().sync_enabled;
    let folder = config
        .read()
        .sync_folder_path
        .clone()
        .unwrap_or_default();
    let has_folder = !folder.is_empty();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Sync" }
            p { class: "settings-description",
                "Sync your library across devices using a shared folder (iCloud Drive, Dropbox, etc.). "
                "Changes are merged automatically using conflict-free replicated data types (CRDTs)."
            }

            // Enable/disable toggle
            div { class: "settings-row",
                label { class: "detail-label", "Enable sync" }
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |evt: Event<FormData>| {
                        let val = evt.checked();
                        config.with_mut(|c| c.sync_enabled = val);
                        match config.read().save() {
                            Ok(()) => {
                                if val {
                                    status_msg.set(Some("Sync enabled.".to_string()));
                                } else {
                                    status_msg.set(Some("Sync disabled.".to_string()));
                                }
                            }
                            Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                        }
                    },
                }
            }

            if enabled {
                // Sync folder path
                div { class: "settings-current-path",
                    span { class: "detail-label", "Sync folder" }
                    if has_folder {
                        div { class: "settings-path-value", "{folder}" }
                    } else {
                        div { class: "settings-path-value settings-path-empty",
                            "No folder selected"
                        }
                    }
                }

                div { class: "settings-actions",
                    button {
                        class: "btn btn--primary",
                        onclick: move |_| {
                            let picked = crate::ui::pick_folder("Choose Sync Folder");
                            if let Some(path) = picked {
                                let path_str = path.to_string_lossy().to_string();
                                config.with_mut(|c| c.sync_folder_path = Some(path_str));
                                match config.read().save() {
                                    Ok(()) => status_msg.set(Some("Sync folder updated.".to_string())),
                                    Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                                }
                            }
                        },
                        "Choose Folder"
                    }

                    if has_folder {
                        button {
                            class: "btn btn--ghost",
                            onclick: move |_| {
                                config.with_mut(|c| c.sync_folder_path = None);
                                match config.read().save() {
                                    Ok(()) => status_msg.set(Some("Sync folder cleared.".to_string())),
                                    Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                                }
                            },
                            "Clear"
                        }
                    }
                }

                if has_folder {
                    // Manual sync button
                    div { class: "settings-actions",
                        button {
                            class: "btn btn--ghost",
                            onclick: move |_| {
                                let db = use_context::<rotero_db::Database>();
                                let cfg = config.read().clone();
                                spawn(async move {
                                    let conn = db.conn();
                                    let site_id = match rotero_db::crr::site_id(conn).await {
                                        Ok(id) => id,
                                        Err(e) => {
                                            status_msg.set(Some(format!("Sync failed: {e}")));
                                            return;
                                        }
                                    };
                                    let folder = cfg.sync_folder_path.as_deref().unwrap_or("");
                                    let engine = crate::sync::file_sync::FileSyncEngine::new(
                                        std::path::PathBuf::from(folder),
                                        site_id,
                                    );
                                    match engine.export_changes(conn).await {
                                        Ok(n) => {
                                            let msg = if n > 0 {
                                                format!("Exported {n} changes.")
                                            } else {
                                                "No new changes to export.".to_string()
                                            };
                                            match engine.import_changes(conn).await {
                                                Ok(m) if m > 0 => {
                                                    status_msg.set(Some(format!("{msg} Imported {m} changes.")));
                                                }
                                                Ok(_) => {
                                                    status_msg.set(Some(format!("{msg} No new remote changes.")));
                                                }
                                                Err(e) => {
                                                    status_msg.set(Some(format!("{msg} Import failed: {e}")));
                                                }
                                            }
                                        }
                                        Err(e) => status_msg.set(Some(format!("Export failed: {e}"))),
                                    }
                                });
                            },
                            "Sync Now"
                        }
                    }
                }
            }

            if let Some(msg) = status_msg.read().as_ref() {
                div { class: "settings-status", "{msg}" }
            }
        }
    }
}

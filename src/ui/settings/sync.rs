use dioxus::prelude::*;

use crate::sync::engine::{SyncConfig, SyncTransport};

#[component]
pub fn SyncSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut status_msg = use_signal(|| None::<String>);

    let enabled = config.read().sync_enabled;
    let transport = config.read().sync_transport.clone();
    let is_cloudkit = transport == SyncTransport::CloudKit;
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
                "Sync your library across devices. "
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
                                status_msg.set(Some(if val { "Sync enabled." } else { "Sync disabled." }.to_string()));
                            }
                            Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                        }
                    },
                }
            }

            if enabled {
                // Transport selector
                div { class: "settings-row",
                    label { class: "detail-label", "Method" }
                    select {
                        value: if is_cloudkit { "cloudkit" } else { "file" },
                        onchange: move |evt: Event<FormData>| {
                            let val = evt.value();
                            let transport = if val == "cloudkit" {
                                SyncTransport::CloudKit
                            } else {
                                SyncTransport::File
                            };
                            config.with_mut(|c| c.sync_transport = transport);
                            let _ = config.read().save();
                        },
                        option { value: "cloudkit", "iCloud (recommended)" }
                        option { value: "file", "Shared folder" }
                    }
                }

                if is_cloudkit {
                    // CloudKit — no folder picker needed
                    p { class: "settings-description",
                        "Syncs automatically via your iCloud account. No setup needed — just enable and your library appears on all your Apple devices."
                    }
                } else {
                    // File-based — show folder picker
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
                                    let _ = config.read().save();
                                    status_msg.set(Some("Sync folder updated.".to_string()));
                                }
                            },
                            "Choose Folder"
                        }

                        if has_folder {
                            button {
                                class: "btn btn--ghost",
                                onclick: move |_| {
                                    config.with_mut(|c| c.sync_folder_path = None);
                                    let _ = config.read().save();
                                    status_msg.set(Some("Sync folder cleared.".to_string()));
                                },
                                "Clear"
                            }
                        }
                    }
                }

                // Sync Now button (works for both transports)
                div { class: "settings-actions",
                    button {
                        class: "btn btn--ghost",
                        onclick: move |_| {
                            status_msg.set(Some("Syncing...".to_string()));
                        },
                        "Sync Now"
                    }
                }
            }

            if let Some(msg) = status_msg.read().as_ref() {
                div { class: "settings-status", "{msg}" }
            }
        }
    }
}

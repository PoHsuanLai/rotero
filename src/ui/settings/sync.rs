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

            div { class: "settings-field",
                span { class: "settings-field-label", "Enable sync" }
                div { class: "settings-field-control",
                    label { class: "settings-toggle",
                        input {
                            r#type: "checkbox",
                            checked: enabled,
                            onchange: move |evt: Event<FormData>| {
                                let val = evt.checked();
                                config.with_mut(|c| c.sync_enabled = val);
                                let _ = config.read().save();
                                status_msg.set(Some(if val { "Sync enabled." } else { "Sync disabled." }.to_string()));
                            },
                        }
                        span { class: "settings-toggle-track",
                            span { class: "settings-toggle-thumb" }
                        }
                    }
                }
            }

            p { class: "settings-hint",
                "Sync your library across devices. Changes are merged automatically using CRDTs."
            }

            if enabled {
                // Transport selector
                div { class: "settings-field",
                    span { class: "settings-field-label", "Method" }
                    div { class: "settings-field-control",
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
                            option { value: "cloudkit", "iCloud" }
                            option { value: "file", "Shared folder" }
                        }
                    }
                }

                if is_cloudkit {
                    p { class: "settings-hint",
                        "Syncs automatically via your iCloud account. No setup needed."
                    }
                } else {
                    // File-based — show folder picker
                    div { class: "settings-field",
                        span { class: "settings-field-label", "Sync folder" }
                        div { class: "settings-field-control",
                            if has_folder {
                                code { class: "settings-bib-path", "{folder}" }
                                button {
                                    class: "btn btn--sm btn--ghost",
                                    onclick: move |_| {
                                        config.with_mut(|c| c.sync_folder_path = None);
                                        let _ = config.read().save();
                                        status_msg.set(Some("Sync folder cleared.".to_string()));
                                    },
                                    "Clear"
                                }
                            } else {
                                button {
                                    class: "btn btn--sm btn--secondary",
                                    onclick: move |_| {
                                        let picked = crate::ui::pick_folder("Choose Sync Folder");
                                        if let Some(path) = picked {
                                            let path_str = path.to_string_lossy().to_string();
                                            config.with_mut(|c| c.sync_folder_path = Some(path_str));
                                            let _ = config.read().save();
                                            status_msg.set(Some("Sync folder updated.".to_string()));
                                        }
                                    },
                                    "Choose folder..."
                                }
                            }
                        }
                    }
                    p { class: "settings-hint",
                        "Point to a cloud-synced folder (iCloud Drive, Dropbox, etc.) to sync changesets between devices."
                    }
                }
            }

            if let Some(msg) = status_msg.read().as_ref() {
                div { class: "settings-status", "{msg}" }
            }
        }
    }
}

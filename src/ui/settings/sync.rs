use dioxus::prelude::*;

use crate::app::DbGeneration;
use crate::sync::engine::{SyncConfig, SyncTransport};

#[component]
fn PathField(
    label: &'static str,
    path: String,
    show_reset: bool,
    on_pick: EventHandler<()>,
    on_clear: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "settings-field",
            span { class: "settings-field-label", "{label}" }
            div { class: "settings-field-control settings-path-control",
                code { class: "settings-bib-path", "{path}" }
                button {
                    class: "btn btn--sm btn--secondary",
                    onclick: move |_| on_pick.call(()),
                    "Change..."
                }
                if show_reset {
                    button {
                        class: "btn btn--sm btn--ghost",
                        onclick: move |_| on_clear.call(()),
                        "Reset"
                    }
                }
            }
        }
    }
}

#[component]
pub fn SyncSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut db_generation = use_context::<DbGeneration>();
    let mut status_msg = use_signal(|| None::<String>);

    let current_path = config
        .read()
        .effective_library_path()
        .to_string_lossy()
        .to_string();
    let is_custom = config.read().sync.library_path.is_some();
    let enabled = config.read().sync.sync_enabled;
    let transport = config.read().sync.sync_transport.clone();
    let is_cloudkit = transport == SyncTransport::CloudKit;
    let folder = config.read().sync.sync_folder_path.clone().unwrap_or_default();
    let has_folder = !folder.is_empty();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Library & Sync" }

            PathField {
                label: "Library location",
                path: current_path,
                show_reset: is_custom,
                on_pick: move |_| {
                    let picked = crate::ui::pick_folder("Choose Library Folder");
                    if let Some(path) = picked {
                        let path_str = path.to_string_lossy().to_string();
                        config.with_mut(|c| c.sync.library_path = Some(path_str));
                        match config.read().save() {
                            Ok(()) => {
                                db_generation.with_mut(|g| *g += 1);
                                status_msg.set(Some("Library path updated.".to_string()));
                            }
                            Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                        }
                    }
                },
                on_clear: move |_| {
                    config.with_mut(|c| c.sync.library_path = None);
                    match config.read().save() {
                        Ok(()) => {
                            db_generation.with_mut(|g| *g += 1);
                            status_msg.set(Some("Reset to default location.".to_string()));
                        }
                        Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                    }
                },
            }
            p { class: "settings-hint", "Where your papers and database are stored." }

            div { class: "settings-field",
                span { class: "settings-field-label", "Sync across devices" }
                div { class: "settings-field-control",
                    label { class: "settings-toggle",
                        input {
                            r#type: "checkbox",
                            checked: enabled,
                            onchange: move |evt: Event<FormData>| {
                                let val = evt.checked();
                                config.with_mut(|c| c.sync.sync_enabled = val);
                                let _ = config.read().save();
                            },
                        }
                        span { class: "settings-toggle-track",
                            span { class: "settings-toggle-thumb" }
                        }
                    }
                }
            }

            if enabled {
                div { class: "settings-field",
                    span { class: "settings-field-label", "Method" }
                    div { class: "settings-field-control",
                        select {
                            class: "select settings-select",
                            value: if is_cloudkit { "cloudkit" } else { "file" },
                            onchange: move |evt: Event<FormData>| {
                                let val = evt.value();
                                let transport = if val == "cloudkit" {
                                    SyncTransport::CloudKit
                                } else {
                                    SyncTransport::File
                                };
                                config.with_mut(|c| c.sync.sync_transport = transport);
                                let _ = config.read().save();
                            },
                            option { value: "cloudkit", "iCloud" }
                            option { value: "file", "Shared folder" }
                        }
                    }
                }

                if is_cloudkit {
                    p { class: "settings-hint",
                        "Syncs via your iCloud account. No setup needed."
                    }
                } else {
                    PathField {
                        label: "Sync folder",
                        path: if has_folder { folder } else { "Not set".to_string() },
                        show_reset: has_folder,
                        on_pick: move |_| {
                            let picked = crate::ui::pick_folder("Choose Sync Folder");
                            if let Some(path) = picked {
                                let path_str = path.to_string_lossy().to_string();
                                config.with_mut(|c| c.sync.sync_folder_path = Some(path_str));
                                let _ = config.read().save();
                            }
                        },
                        on_clear: move |_| {
                            config.with_mut(|c| c.sync.sync_folder_path = None);
                            let _ = config.read().save();
                        },
                    }
                    p { class: "settings-hint",
                        "Point to a cloud-synced folder (iCloud Drive, Dropbox, etc.)."
                    }
                }
            }

            if let Some(msg) = status_msg.read().as_ref() {
                div { class: "settings-status", "{msg}" }
            }
        }
    }
}

use dioxus::prelude::*;

use crate::db::Database;
use crate::sync::engine::SyncConfig;

#[component]
pub fn SettingsButton() -> Element {
    let mut show = use_signal(|| false);

    rsx! {
        button {
            class: "sidebar-settings-btn",
            onclick: move |_| show.set(!show()),
            "Settings"
        }
        if show() {
            SettingsPanel { on_close: move || show.set(false) }
        }
    }
}

#[component]
fn SettingsPanel(on_close: EventHandler<()>) -> Element {
    let db = use_context::<Database>();
    let mut config = use_signal(|| SyncConfig::load());
    let mut status_msg = use_signal(|| None::<String>);

    let current_path = config.read().effective_library_path().to_string_lossy().to_string();
    let is_custom = config.read().library_path.is_some();

    rsx! {
        div { class: "settings-overlay",
            onclick: move |_| on_close.call(()),

            div { class: "settings-dialog",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "settings-header",
                    h3 { "Settings" }
                    button {
                        class: "detail-close",
                        onclick: move |_| on_close.call(()),
                        "\u{00d7}"
                    }
                }

                // Library location
                div { class: "settings-section",
                    h4 { class: "settings-section-title", "Library Location" }
                    p { class: "settings-description",
                        "Set your library folder to a cloud-synced directory (Dropbox, iCloud Drive, OneDrive) to sync across devices."
                    }

                    div { class: "settings-current-path",
                        span { class: "detail-label", "Current location" }
                        div { class: "settings-path-value", "{current_path}" }
                    }

                    div { class: "settings-actions",
                        button {
                            class: "btn btn--primary",
                            onclick: move |_| {
                                let folder = rfd::FileDialog::new()
                                    .set_title("Choose Library Folder")
                                    .pick_folder();

                                if let Some(path) = folder {
                                    let path_str = path.to_string_lossy().to_string();
                                    config.with_mut(|c| c.library_path = Some(path_str));
                                    match config.read().save() {
                                        Ok(()) => status_msg.set(Some("Library path updated. Restart to apply.".to_string())),
                                        Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                                    }
                                }
                            },
                            "Change Location"
                        }

                        if is_custom {
                            button {
                                class: "btn btn--ghost",
                                onclick: move |_| {
                                    config.with_mut(|c| c.library_path = None);
                                    match config.read().save() {
                                        Ok(()) => status_msg.set(Some("Reset to default. Restart to apply.".to_string())),
                                        Err(e) => status_msg.set(Some(format!("Failed to save: {e}"))),
                                    }
                                },
                                "Reset to Default"
                            }
                        }
                    }

                    if let Some(msg) = status_msg.read().as_ref() {
                        div { class: "settings-status", "{msg}" }
                    }
                }

                // Info section
                div { class: "settings-section",
                    h4 { class: "settings-section-title", "About" }
                    p { class: "settings-description", "Rotero v0.1.0 — A lightweight paper reader" }
                    p { class: "settings-description", "Database: turso (pure Rust SQLite)" }
                }
            }
        }
    }
}

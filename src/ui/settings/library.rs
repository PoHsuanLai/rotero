use dioxus::prelude::*;

use crate::app::DbGeneration;
use crate::sync::engine::SyncConfig;

#[component]
pub fn LibrarySection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut db_generation = use_context::<DbGeneration>();
    let mut status_msg = use_signal(|| None::<String>);

    let current_path = config
        .read()
        .effective_library_path()
        .to_string_lossy()
        .to_string();
    let is_custom = config.read().library_path.is_some();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Library" }
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
                        let folder = crate::ui::pick_folder("Choose Library Folder");

                        if let Some(path) = folder {
                            let path_str = path.to_string_lossy().to_string();
                            config.with_mut(|c| c.library_path = Some(path_str));
                            match config.read().save() {
                                Ok(()) => {
                                    // Bump generation to trigger DB re-init without restart
                                    db_generation.with_mut(|g| *g += 1);
                                    status_msg.set(Some("Library path updated and reloaded.".to_string()));
                                }
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
                                Ok(()) => {
                                    db_generation.with_mut(|g| *g += 1);
                                    status_msg.set(Some("Reset to default and reloaded.".to_string()));
                                }
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
    }
}

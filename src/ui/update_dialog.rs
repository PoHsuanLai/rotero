use dioxus::prelude::*;

use crate::updates::{UpdateState, UpdateStatus, apply_update};

fn restart_app() {
    // Use the bundle identifier to relaunch via macOS `open -b`, which avoids
    // opening a Terminal window. This works even after the .app bundle has been
    // replaced on disk (the old exe path may no longer exist).
    let launched = std::process::Command::new("open")
        .arg("-b")
        .arg("com.rotero.Rotero")
        .spawn()
        .is_ok();

    if !launched {
        // Fallback: try to find and open the .app bundle directly.
        if let Ok(exe) = std::env::current_exe() {
            let mut path = exe.as_path();
            while let Some(parent) = path.parent() {
                if path.extension().and_then(|e| e.to_str()) == Some("app") {
                    let _ = std::process::Command::new("open").arg(path).spawn();
                    break;
                }
                path = parent;
            }
        }
    }

    std::process::exit(0);
}

#[component]
pub fn UpdateDialog() -> Element {
    let mut update_state = use_context::<Signal<UpdateState>>();

    if !update_state.read().show_dialog {
        return rsx! {};
    }

    let status = update_state.read().status;

    rsx! {
        div { class: "settings-overlay",
            onclick: move |_| {
                update_state.with_mut(|s| s.show_dialog = false);
            },

            div { class: "settings-dialog update-dialog",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "settings-header",
                    h3 {
                        match status {
                            UpdateStatus::Checking => "Checking for Updates",
                            UpdateStatus::Available => "Update Available",
                            UpdateStatus::Downloading => "Downloading Update",
                            UpdateStatus::ReadyToRestart => "Update Installed",
                            UpdateStatus::UpToDate => "Up to Date",
                            UpdateStatus::Error => "Update Error",
                            UpdateStatus::Idle => "Updates",
                        }
                    }
                    button {
                        class: "detail-close",
                        onclick: move |_| {
                            update_state.with_mut(|s| s.show_dialog = false);
                        },
                        "\u{00d7}"
                    }
                }

                div { class: "settings-tab-content",
                    match status {
                        UpdateStatus::Checking => rsx! {
                            div { class: "settings-section",
                                p { class: "settings-description", "Checking for updates\u{2026}" }
                            }
                        },
                        UpdateStatus::Available => {
                            let info = update_state.read().info.clone();
                            let (latest, notes, dl_url) = match &info {
                                Some(i) => (i.latest_version.clone(), i.release_notes.clone(), i.download_url.clone()),
                                None => (String::new(), String::new(), String::new()),
                            };
                            rsx! {
                                div { class: "settings-section",
                                    p { class: "settings-description",
                                        "A new version of Rotero is available: "
                                        strong { "v{latest}" }
                                    }
                                    p { class: "settings-description",
                                        "Current version: v{env!(\"CARGO_PKG_VERSION\")}"
                                    }
                                    if !notes.is_empty() {
                                        div { class: "settings-section",
                                            h4 { class: "settings-section-title", "Release Notes" }
                                            p { class: "settings-description", "{notes}" }
                                        }
                                    }
                                    div { class: "settings-section", style: "display: flex; gap: 8px; margin-top: 16px;",
                                        button {
                                            class: "btn btn--primary",
                                            onclick: move |_| {
                                                let url = dl_url.clone();
                                                update_state.with_mut(|s| s.status = UpdateStatus::Downloading);
                                                spawn(async move {
                                                    match apply_update(&url).await {
                                                        Ok(()) => {
                                                            update_state.with_mut(|s| {
                                                                s.status = UpdateStatus::ReadyToRestart;
                                                            });
                                                        }
                                                        Err(e) => {
                                                            update_state.with_mut(|s| {
                                                                s.status = UpdateStatus::Error;
                                                                s.error = Some(e);
                                                            });
                                                        }
                                                    }
                                                });
                                            },
                                            "Download & Install"
                                        }
                                        button {
                                            class: "btn",
                                            onclick: move |_| {
                                                update_state.with_mut(|s| s.show_dialog = false);
                                            },
                                            "Later"
                                        }
                                    }
                                }
                            }
                        },
                        UpdateStatus::Downloading => rsx! {
                            div { class: "settings-section",
                                p { class: "settings-description", "Downloading and installing update\u{2026}" }
                            }
                        },
                        UpdateStatus::ReadyToRestart => rsx! {
                            div { class: "settings-section",
                                p { class: "settings-description",
                                    "Update installed successfully. Restart Rotero to apply."
                                }
                                div { style: "display: flex; gap: 8px; margin-top: 16px;",
                                    button {
                                        class: "btn btn--primary",
                                        onclick: move |_| {
                                            restart_app();
                                        },
                                        "Restart Now"
                                    }
                                    button {
                                        class: "btn",
                                        onclick: move |_| {
                                            update_state.with_mut(|s| s.show_dialog = false);
                                        },
                                        "Later"
                                    }
                                }
                            }
                        },
                        UpdateStatus::UpToDate => rsx! {
                            div { class: "settings-section",
                                p { class: "settings-description",
                                    "You're running the latest version (v{env!(\"CARGO_PKG_VERSION\")})."
                                }
                                div { style: "margin-top: 16px;",
                                    button {
                                        class: "btn btn--primary",
                                        onclick: move |_| {
                                            update_state.with_mut(|s| s.show_dialog = false);
                                        },
                                        "OK"
                                    }
                                }
                            }
                        },
                        UpdateStatus::Error => {
                            let err = update_state.read().error.clone().unwrap_or_default();
                            rsx! {
                                div { class: "settings-section",
                                    p { class: "settings-description", "Failed to check for updates:" }
                                    p { class: "settings-description", style: "color: var(--color-error, #ef4444);",
                                        "{err}"
                                    }
                                    div { style: "margin-top: 16px;",
                                        button {
                                            class: "btn btn--primary",
                                            onclick: move |_| {
                                                update_state.with_mut(|s| s.show_dialog = false);
                                            },
                                            "OK"
                                        }
                                    }
                                }
                            }
                        },
                        UpdateStatus::Idle => rsx! {},
                    }
                }
            }
        }
    }
}

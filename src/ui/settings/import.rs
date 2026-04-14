use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::ui::components::settings_field::SettingsField;
use crate::ui::components::toggle_switch::ToggleSwitch;
use crate::ui::helpers::save_config;

#[component]
pub fn ImportSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let auto_fetch = config.read().auto_fetch_metadata;

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Import & Metadata" }

            SettingsField { label: "Auto-fetch metadata on import",
                ToggleSwitch {
                    checked: auto_fetch,
                    onchange: move |checked| {
                        save_config(&mut config, |c| c.auto_fetch_metadata = checked);
                    },
                }
            }

            p { class: "settings-hint", "When enabled, Rotero fetches paper metadata from CrossRef after importing a PDF." }

            SettingsField { label: "Auto-export .bib file",
                {
                    let bib_path = config.read().sync.auto_export_bib_path.clone();
                    let has_path = bib_path.is_some();
                    let display_path = bib_path.unwrap_or_default();
                    rsx! {
                        if has_path {
                            code { class: "settings-bib-path", "{display_path}" }
                            button {
                                class: "btn btn--sm btn--ghost",
                                onclick: move |_| {
                                    save_config(&mut config, |c| c.sync.auto_export_bib_path = None);
                                },
                                "Clear"
                            }
                        } else {
                            button {
                                class: "btn btn--sm btn--secondary",
                                onclick: move |_| {
                                    #[cfg(feature = "desktop")]
                                    spawn(async move {
                                        use rfd::AsyncFileDialog;
                                        if let Some(file) = AsyncFileDialog::new()
                                            .set_file_name("rotero-library.bib")
                                            .add_filter("BibTeX", &["bib"])
                                            .save_file()
                                            .await
                                        {
                                            let path = file.path().to_string_lossy().to_string();
                                            save_config(&mut config, |c| c.sync.auto_export_bib_path = Some(path));
                                        }
                                    });
                                },
                                "Choose path..."
                            }
                        }
                    }
                }
            }
            p { class: "settings-hint", "When set, Rotero automatically keeps a .bib file in sync with your library. Citation keys are auto-generated (author+year+title) and can be customized per paper." }
        }
    }
}

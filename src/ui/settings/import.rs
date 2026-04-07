use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;

#[component]
pub fn ImportSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let auto_fetch = config.read().auto_fetch_metadata;

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Import & Metadata" }

            div { class: "settings-field",
                span { class: "settings-field-label", "Auto-fetch metadata on import" }
                div { class: "settings-field-control",
                    label { class: "settings-toggle",
                        input {
                            r#type: "checkbox",
                            checked: auto_fetch,
                            onchange: move |evt| {
                                let checked = evt.checked();
                                config.with_mut(|c| c.auto_fetch_metadata = checked);
                                if let Err(e) = config.read().save() {
                                    tracing::error!("Failed to save config: {e}");
                                }
                            },
                        }
                        span { class: "settings-toggle-track",
                            span { class: "settings-toggle-thumb" }
                        }
                    }
                }
            }

            p { class: "settings-hint", "When enabled, Rotero fetches paper metadata from CrossRef after importing a PDF." }

            div { class: "settings-field",
                span { class: "settings-field-label", "Auto-export .bib file" }
                div { class: "settings-field-control",
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
                                        config.with_mut(|c| c.sync.auto_export_bib_path = None);
                                        if let Err(e) = config.read().save() {
                                    tracing::error!("Failed to save config: {e}");
                                }
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
                                                config.with_mut(|c| c.sync.auto_export_bib_path = Some(path));
                                                if let Err(e) = config.read().save() {
                                    tracing::error!("Failed to save config: {e}");
                                }
                                            }
                                        });
                                    },
                                    "Choose path..."
                                }
                            }
                        }
                    }
                }
            }
            p { class: "settings-hint", "When set, Rotero automatically keeps a .bib file in sync with your library. Citation keys are auto-generated (author+year+title) and can be customized per paper." }
        }
    }
}

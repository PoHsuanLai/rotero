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
                                let _ = config.read().save();
                            },
                        }
                        span { class: "settings-toggle-track",
                            span { class: "settings-toggle-thumb" }
                        }
                    }
                }
            }

            p { class: "settings-hint", "When enabled, Rotero fetches paper metadata from CrossRef after importing a PDF." }
        }
    }
}

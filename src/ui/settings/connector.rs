use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;

#[component]
pub fn ConnectorSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let enabled = config.read().connector.connector_enabled;
    let port = config.read().connector.connector_port;

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Browser Connector" }

            div { class: "settings-field",
                span { class: "settings-field-label", "Enabled" }
                div { class: "settings-field-control",
                    label { class: "settings-toggle",
                        input {
                            r#type: "checkbox",
                            checked: enabled,
                            onchange: move |evt| {
                                let checked = evt.checked();
                                config.with_mut(|c| c.connector.connector_enabled = checked);
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
                    span { class: "settings-field-label", "Port" }
                    div { class: "settings-field-control",
                        input {
                            class: "input input--sm settings-number-input",
                            r#type: "number",
                            value: "{port}",
                            min: "1024",
                            max: "65535",
                            onchange: move |evt| {
                                if let Ok(p) = evt.value().parse::<u16>() {
                                    config.with_mut(|c| c.connector.connector_port = p);
                                    let _ = config.read().save();
                                }
                            },
                        }
                    }
                }
            }

            p { class: "settings-hint", "Changes take effect on restart." }
        }
    }
}

use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::ui::components::settings_field::SettingsField;
use crate::ui::components::toggle_switch::ToggleSwitch;
use crate::ui::helpers::save_config;

#[component]
pub fn ConnectorSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let enabled = config.read().connector.connector_enabled;
    let port = config.read().connector.connector_port;

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Browser Connector" }

            SettingsField { label: "Enabled",
                ToggleSwitch {
                    checked: enabled,
                    onchange: move |checked| {
                        save_config(&mut config, |c| c.connector.connector_enabled = checked);
                    },
                }
            }

            if enabled {
                SettingsField { label: "Port",
                    input {
                        class: "input input--sm settings-number-input",
                        r#type: "number",
                        value: "{port}",
                        min: "1024",
                        max: "65535",
                        onchange: move |evt| {
                            if let Ok(p) = evt.value().parse::<u16>() {
                                save_config(&mut config, |c| c.connector.connector_port = p);
                            }
                        },
                    }
                }
            }

            p { class: "settings-hint", "Changes take effect on restart." }
        }
    }
}

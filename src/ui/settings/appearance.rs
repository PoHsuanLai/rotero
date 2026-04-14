use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::ui::components::settings_field::SettingsField;
use crate::ui::components::toggle_switch::ToggleSwitch;
use crate::ui::helpers::save_config;

const SCALE_OPTIONS: &[(&str, &str)] = &[
    ("compact", "Compact"),
    ("default", "Default"),
    ("comfortable", "Comfortable"),
];

#[component]
pub fn AppearanceSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let dark_mode = config.read().ui.dark_mode;
    let current_scale = config.read().ui.ui_scale.clone();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Appearance" }

            SettingsField { label: "Dark mode",
                ToggleSwitch {
                    checked: dark_mode,
                    onchange: move |checked| {
                        save_config(&mut config, |c| c.ui.dark_mode = checked);
                    },
                }
            }

            SettingsField { label: "UI density",
                div { class: "settings-radio-group",
                    for (value, label) in SCALE_OPTIONS.iter() {
                        {
                            let v = value.to_string();
                            let v2 = v.clone();
                            let is_active = v == current_scale;
                            let btn_class = if is_active {
                                "settings-radio-btn settings-radio-btn--active"
                            } else {
                                "settings-radio-btn"
                            };
                            rsx! {
                                button {
                                    class: "{btn_class}",
                                    onclick: move |_| {
                                        let scale = v2.clone();
                                        save_config(&mut config, |c| c.ui.ui_scale = scale);
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

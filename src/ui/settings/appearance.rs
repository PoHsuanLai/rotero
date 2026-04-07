use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;

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

            // Dark mode toggle
            div { class: "settings-field",
                span { class: "settings-field-label", "Dark mode" }
                div { class: "settings-field-control",
                    label { class: "settings-toggle",
                        input {
                            r#type: "checkbox",
                            checked: dark_mode,
                            onchange: move |evt| {
                                let checked = evt.checked();
                                config.with_mut(|c| c.ui.dark_mode = checked);
                                let _ = config.read().save();
                            },
                        }
                        span { class: "settings-toggle-track",
                            span { class: "settings-toggle-thumb" }
                        }
                    }
                }
            }

            // UI density
            div { class: "settings-field",
                span { class: "settings-field-label", "UI density" }
                div { class: "settings-field-control",
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
                                            config.with_mut(|c| c.ui.ui_scale = scale);
                                            let _ = config.read().save();
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
}

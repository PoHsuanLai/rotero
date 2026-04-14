use dioxus::prelude::*;

use crate::state::app_state::PdfTabManager;
use crate::sync::engine::SyncConfig;
use crate::ui::components::settings_field::SettingsField;
use crate::ui::components::settings_select::SettingsSelect;
use crate::ui::helpers::save_config;

const ZOOM_OPTIONS: &[(f32, &str)] = &[
    (0.5, "50%"),
    (0.75, "75%"),
    (1.0, "100%"),
    (1.5, "150%"),
    (2.0, "200%"),
    (3.0, "300%"),
];

const BATCH_OPTIONS: &[(u32, &str)] = &[
    (3, "3 pages"),
    (5, "5 pages"),
    (10, "10 pages"),
    (20, "20 pages"),
];

const SELECTION_COLORS: &[(&str, &str)] = &[
    ("#ffff00", "Yellow"),
    ("#ff6b6b", "Red"),
    ("#51cf66", "Green"),
    ("#339af0", "Blue"),
    ("#cc5de8", "Purple"),
    ("#ff922b", "Orange"),
];

#[component]
pub fn PdfViewerSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let current_zoom = config.read().pdf.default_zoom;
    let current_batch = config.read().pdf.page_batch_size;
    let current_resident = config.read().max_resident_tabs;
    let current_color = config.read().pdf.selection_color.clone();

    let zoom_opts: Vec<(String, String)> = ZOOM_OPTIONS
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect();
    let batch_opts: Vec<(String, String)> = BATCH_OPTIONS
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "PDF Viewer" }

            SettingsField { label: "Default zoom",
                SettingsSelect {
                    value: current_zoom.to_string(),
                    options: zoom_opts,
                    onchange: move |val: String| {
                        if let Ok(z) = val.parse::<f32>() {
                            save_config(&mut config, |c| c.pdf.default_zoom = z);
                        }
                    },
                }
            }

            SettingsField { label: "Selection color",
                div { class: "settings-color-picker",
                    for (color, _name) in SELECTION_COLORS.iter() {
                        {
                            let c = color.to_string();
                            let c2 = c.clone();
                            let is_selected = c == current_color;
                            let swatch_class = if is_selected {
                                "color-swatch color-swatch--selected"
                            } else {
                                "color-swatch"
                            };
                            rsx! {
                                div {
                                    class: "{swatch_class}",
                                    style: "background: {c};",
                                    onclick: move |_| {
                                        let color = c2.clone();
                                        save_config(&mut config, |c| c.pdf.selection_color = color);
                                    },
                                }
                            }
                        }
                    }
                }
            }

            SettingsField { label: "Pages to preload",
                SettingsSelect {
                    value: current_batch.to_string(),
                    options: batch_opts,
                    onchange: move |val: String| {
                        if let Ok(b) = val.parse::<u32>() {
                            save_config(&mut config, |c| c.pdf.page_batch_size = b);
                        }
                    },
                }
            }

            SettingsField { label: "Tabs cached in memory",
                input {
                    r#type: "number",
                    class: "input settings-input",
                    value: "{current_resident}",
                    min: "1",
                    max: "50",
                    onchange: move |evt| {
                        if let Ok(v) = evt.value().parse::<u32>() {
                            let v = v.clamp(1, 50);
                            save_config(&mut config, |c| c.max_resident_tabs = v);
                            tabs.with_mut(|m| m.set_max_resident(v));
                        }
                    },
                }
            }
        }
    }
}

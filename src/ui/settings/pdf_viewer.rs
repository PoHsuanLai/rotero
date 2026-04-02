use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;

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

const ANNOTATION_COLORS: &[(&str, &str)] = &[
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
    let current_zoom = config.read().default_zoom;
    let current_batch = config.read().page_batch_size;
    let current_color = config.read().default_annotation_color.clone();

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "PDF Viewer" }

            // Default zoom
            div { class: "settings-field",
                span { class: "settings-field-label", "Default zoom" }
                div { class: "settings-field-control",
                    select {
                        class: "select settings-select",
                        value: "{current_zoom}",
                        onchange: move |evt| {
                            if let Ok(z) = evt.value().parse::<f32>() {
                                config.with_mut(|c| c.default_zoom = z);
                                let _ = config.read().save();
                            }
                        },
                        for (val, label) in ZOOM_OPTIONS.iter() {
                            option { value: "{val}", "{label}" }
                        }
                    }
                }
            }

            // Default annotation color
            div { class: "settings-field",
                span { class: "settings-field-label", "Highlight color" }
                div { class: "settings-field-control",
                    div { class: "settings-color-picker",
                        for (color, _name) in ANNOTATION_COLORS.iter() {
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
                                            config.with_mut(|c| c.default_annotation_color = color);
                                            let _ = config.read().save();
                                        },
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Page batch size
            div { class: "settings-field",
                span { class: "settings-field-label", "Pages to preload" }
                div { class: "settings-field-control",
                    select {
                        class: "select settings-select",
                        value: "{current_batch}",
                        onchange: move |evt| {
                            if let Ok(b) = evt.value().parse::<u32>() {
                                config.with_mut(|c| c.page_batch_size = b);
                                let _ = config.read().save();
                            }
                        },
                        for (val, label) in BATCH_OPTIONS.iter() {
                            option { value: "{val}", "{label}" }
                        }
                    }
                }
            }
        }
    }
}

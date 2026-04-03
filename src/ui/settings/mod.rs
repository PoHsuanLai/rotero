mod appearance;
mod connector;
mod import;
mod library;
mod pdf_viewer;

use crate::app::ShowSettings;
use dioxus::prelude::*;

#[component]
pub fn SettingsButton() -> Element {
    let mut show = use_context::<Signal<ShowSettings>>();

    rsx! {
        button {
            class: "sidebar-settings-btn",
            onclick: move |_| {
                let current = show.read().0;
                show.set(ShowSettings(!current));
            },
            "Settings"
        }
        if show.read().0 {
            SettingsPanel { on_close: move || show.set(ShowSettings(false)) }
        }
    }
}

#[component]
fn SettingsPanel(on_close: EventHandler<()>) -> Element {
    rsx! {
        div { class: "settings-overlay",
            onclick: move |_| on_close.call(()),

            div { class: "settings-dialog",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "settings-header",
                    h3 { "Settings" }
                    button {
                        class: "detail-close",
                        onclick: move |_| on_close.call(()),
                        "\u{00d7}"
                    }
                }

                library::LibrarySection {}

                div { class: "settings-divider" }
                pdf_viewer::PdfViewerSection {}

                div { class: "settings-divider" }
                appearance::AppearanceSection {}

                div { class: "settings-divider" }
                connector::ConnectorSection {}

                div { class: "settings-divider" }
                import::ImportSection {}

                div { class: "settings-divider" }

                // About
                div { class: "settings-section",
                    h4 { class: "settings-section-title", "About" }
                    p { class: "settings-description", "Rotero v0.1.0" }
                    p { class: "settings-description", "Database: turso (pure Rust SQLite)" }
                }
            }
        }
    }
}

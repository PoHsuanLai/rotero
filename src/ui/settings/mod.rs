mod appearance;
mod claude;
mod connector;
mod import;
mod pdf_viewer;
mod sync;
#[cfg(feature = "desktop")]
mod update;

use crate::app::ShowSettings;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    General,
    PdfViewer,
    AiAgent,
    Advanced,
}

impl SettingsTab {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::PdfViewer => "PDF Viewer",
            Self::AiAgent => "AI Agent",
            Self::Advanced => "Advanced",
        }
    }
}

const TABS: [SettingsTab; 4] = [
    SettingsTab::General,
    SettingsTab::PdfViewer,
    SettingsTab::AiAgent,
    SettingsTab::Advanced,
];

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
    let mut active_tab = use_signal(|| SettingsTab::General);

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

                div { class: "settings-tabs",
                    for tab in TABS {
                        button {
                            class: if *active_tab.read() == tab { "settings-tab settings-tab--active" } else { "settings-tab" },
                            onclick: move |_| active_tab.set(tab),
                            "{tab.label()}"
                        }
                    }
                }

                div { class: "settings-tab-content",
                    match *active_tab.read() {
                        SettingsTab::General => rsx! {
                            sync::SyncSection {}
                            div { class: "settings-divider" }
                            appearance::AppearanceSection {}
                            div { class: "settings-divider" }
                            import::ImportSection {}
                        },
                        SettingsTab::PdfViewer => rsx! {
                            pdf_viewer::PdfViewerSection {}
                        },
                        SettingsTab::AiAgent => rsx! {
                            claude::AgentSection {}
                        },
                        SettingsTab::Advanced => rsx! {
                            connector::ConnectorSection {}
                            div { class: "settings-divider" }
                            {update_settings_element()}
                            div { class: "settings-section",
                                h4 { class: "settings-section-title", "About" }
                                p { class: "settings-description",
                                    "Rotero v{env!(\"CARGO_PKG_VERSION\")}"
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}

#[cfg(feature = "desktop")]
fn update_settings_element() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    rsx! {
        update::UpdateSection {}
        div { class: "settings-divider" }
    }
}

#[cfg(not(feature = "desktop"))]
fn update_settings_element() -> dioxus::prelude::Element {
    use dioxus::prelude::*;
    rsx! {}
}

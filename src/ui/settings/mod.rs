mod appearance;
mod claude;
mod connector;
mod import;
// Keybindings settings depend on `ui::keybindings`, which is desktop-only.
#[cfg(feature = "desktop")]
mod keybindings;
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
    #[cfg(feature = "desktop")]
    Keybindings,
    AiAgent,
    Connector,
    About,
}

impl SettingsTab {
    fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::PdfViewer => "PDF Viewer",
            #[cfg(feature = "desktop")]
            Self::Keybindings => "Keybindings",
            Self::AiAgent => "AI Agent",
            Self::Connector => "Connector",
            Self::About => "About",
        }
    }
}

const TABS: &[SettingsTab] = &[
    SettingsTab::General,
    SettingsTab::PdfViewer,
    #[cfg(feature = "desktop")]
    SettingsTab::Keybindings,
    SettingsTab::AiAgent,
    SettingsTab::Connector,
    SettingsTab::About,
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

                div { class: "settings-body",

                div { class: "settings-tabs",
                    for tab in TABS.iter().copied() {
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
                        #[cfg(feature = "desktop")]
                        SettingsTab::Keybindings => rsx! {
                            keybindings::KeybindingsSection {}
                        },
                        SettingsTab::AiAgent => rsx! {
                            claude::AgentSection {}
                        },
                        SettingsTab::Connector => rsx! {
                            connector::ConnectorSection {}
                        },
                        SettingsTab::About => rsx! {
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

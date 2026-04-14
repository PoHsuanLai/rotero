use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::ui::components::settings_field::SettingsField;
use crate::ui::components::toggle_switch::ToggleSwitch;
use crate::ui::helpers::save_config;
use crate::updates::{UpdateState, UpdateStatus};

#[component]
pub fn UpdateSection() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut update_state = use_context::<Signal<UpdateState>>();
    let enabled = config.read().update.auto_check_updates;
    let checking = update_state.read().status == UpdateStatus::Checking;

    rsx! {
        div { class: "settings-section",
            h4 { class: "settings-section-title", "Updates" }

            SettingsField { label: "Check automatically",
                ToggleSwitch {
                    checked: enabled,
                    onchange: move |checked| {
                        save_config(&mut config, |c| c.update.auto_check_updates = checked);
                    },
                }
            }

            SettingsField { label: "",
                button {
                    class: "btn btn--sm",
                    disabled: checking,
                    onclick: move |_| {
                        update_state.with_mut(|s| {
                            s.status = UpdateStatus::Checking;
                            s.show_dialog = true;
                            s.error = None;
                        });
                        spawn(async move {
                            match crate::updates::check_for_update().await {
                                Ok(Some(info)) => {
                                    update_state.with_mut(|s| {
                                        s.status = UpdateStatus::Available;
                                        s.info = Some(info);
                                    });
                                }
                                Ok(None) => {
                                    update_state.with_mut(|s| {
                                        s.status = UpdateStatus::UpToDate;
                                    });
                                }
                                Err(e) => {
                                    update_state.with_mut(|s| {
                                        s.status = UpdateStatus::Error;
                                        s.error = Some(e);
                                    });
                                }
                            }
                        });
                    },
                    if checking { "Checking\u{2026}" } else { "Check Now" }
                }
            }
        }
    }
}

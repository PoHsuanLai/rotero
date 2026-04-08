use std::time::Duration;

use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::updates::{UpdateState, UpdateStatus, check_for_update};

#[component]
pub fn UpdateChecker() -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let mut update_state = use_context::<Signal<UpdateState>>();

    use_future(move || async move {
        // Wait a bit after startup before checking.
        tokio::time::sleep(Duration::from_secs(5)).await;

        if !config.read().update.auto_check_updates {
            return;
        }

        // Always check once on startup.
        match check_for_update().await {
            Ok(Some(info)) => {
                update_state.with_mut(|s| {
                    s.status = UpdateStatus::Available;
                    s.info = Some(info);
                    s.show_dialog = true;
                });
            }
            Ok(None) => {}
            Err(e) => {
                tracing::debug!("Auto update check failed: {e}");
            }
        }
        config.with_mut(|c| {
            c.update.last_check_timestamp = Some(chrono::Utc::now().timestamp());
        });
        if let Err(e) = config.read().save() {
            tracing::error!("Failed to save config after update check: {e}");
        }
    });

    rsx! {}
}

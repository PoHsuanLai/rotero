//! In-app updates: check GitHub Releases, download the platform artifact, and
//! replace the running install.

mod check;
mod exe;
mod install;
mod types;

pub use check::{RELEASES_PAGE, check_for_update};
pub use exe::{remember_running_exe, running_exe};
pub use types::{UpdateError, UpdateState, UpdateStatus};

/// Run a user-initiated update check, driving `state` through the dialog.
///
/// Shared by the Settings button and the Help-menu command so the two can't
/// drift apart. The startup check in `app::update_checker` deliberately keeps
/// its own quieter handling: it must not pop a dialog on failure.
pub fn run_interactive_check(mut state: dioxus::prelude::Signal<UpdateState>) {
    use dioxus::prelude::*;

    state.with_mut(|s| {
        s.status = UpdateStatus::Checking;
        s.show_dialog = true;
        s.error = None;
    });
    spawn(async move {
        match check_for_update().await {
            Ok(Some(info)) => state.with_mut(|s| {
                s.status = UpdateStatus::Available;
                s.info = Some(info);
            }),
            Ok(None) => state.with_mut(|s| s.status = UpdateStatus::UpToDate),
            Err(e) => state.with_mut(|s| {
                s.status = UpdateStatus::Error;
                s.error = Some(e);
            }),
        }
    });
}

/// Download the new build and put it in place, ready for a restart.
///
/// The install step differs by platform because the artifacts differ in kind:
/// macOS ships a `.app` *directory* that has to be swapped whole; Windows
/// replaces the executable inside a zip; Linux extracts the portable tarball
/// (binary, optional `.so` sidecars, desktop file, icons) over a user install.
pub async fn apply_update(download_url: &str) -> Result<(), UpdateError> {
    let bytes = reqwest::Client::new()
        .get(download_url)
        .header("User-Agent", "rotero-updater")
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?
        .bytes()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    install::install_update(&bytes)
}

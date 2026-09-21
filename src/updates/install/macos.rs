//! `.app` bundle swap used by the in-app updater on macOS.

use std::path::PathBuf;

use crate::updates::running_exe;
use crate::updates::types::UpdateError;

/// Extract the `.app` and swap it for the running bundle.
pub(crate) fn install_update(bytes: &[u8]) -> Result<(), UpdateError> {
    let app_bundle = current_app_bundle()?;

    let tmp_dir = tempfile::tempdir()
        .map_err(|e| UpdateError::Install(format!("Failed to create temp dir: {e}")))?;
    let zip_path = tmp_dir.path().join("update.zip");
    std::fs::write(&zip_path, bytes)
        .map_err(|e| UpdateError::Install(format!("Failed to write zip: {e}")))?;

    // ditto rather than an in-process unzip: it preserves macOS metadata,
    // resource forks, and the code signature, which a plain zip reader drops.
    let extract_dir = tmp_dir.path().join("extracted");
    std::fs::create_dir_all(&extract_dir)
        .map_err(|e| UpdateError::Install(format!("Failed to create extract dir: {e}")))?;

    let status = std::process::Command::new("ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&extract_dir)
        .status()
        .map_err(|e| UpdateError::Install(format!("ditto failed: {e}")))?;
    if !status.success() {
        return Err(UpdateError::Install("ditto extraction failed".into()));
    }

    let new_app = find_app_in_dir(&extract_dir)?;

    // Swap the bundle: move the current one aside, move the new one in, and
    // restore the old one if that second move fails.
    let backup = app_bundle.with_extension("app.old");
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|e| UpdateError::Install(format!("Failed to remove old backup: {e}")))?;
    }
    std::fs::rename(&app_bundle, &backup)
        .map_err(|e| UpdateError::Install(format!("Failed to move current app: {e}")))?;

    if let Err(e) = std::fs::rename(&new_app, &app_bundle) {
        let _ = std::fs::rename(&backup, &app_bundle);
        return Err(UpdateError::Install(format!(
            "Failed to install new app: {e}"
        )));
    }

    let _ = std::fs::remove_dir_all(&backup);

    // Keep tmp_dir alive until we're done (it auto-deletes on drop).
    // Leak it so it persists through relaunch.
    std::mem::forget(tmp_dir);

    tracing::info!("Update installed to {}", app_bundle.display());
    Ok(())
}

/// Get the path to the current .app bundle (e.g. /Applications/Rotero.app).
fn current_app_bundle() -> Result<PathBuf, UpdateError> {
    // A dev build (`cargo run` / `dx serve`) has no bundle, so this is the
    // usual reason an update can't be applied in place.
    let not_bundled =
        || UpdateError::NotInstalled("This copy isn't running from a Rotero.app bundle.".into());

    let exe =
        running_exe().ok_or_else(|| UpdateError::NotInstalled("Can't find current exe.".into()))?;
    // exe is like /path/to/Rotero.app/Contents/MacOS/rotero
    // Walk up to find the .app directory.
    let mut path = exe.as_path();
    loop {
        path = path.parent().ok_or_else(not_bundled)?;
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            return Ok(path.to_path_buf());
        }
        if path.parent().is_none() {
            return Err(not_bundled());
        }
    }
}

fn find_app_in_dir(dir: &std::path::Path) -> Result<PathBuf, UpdateError> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| UpdateError::Install(format!("Failed to read extract dir: {e}")))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.extension().and_then(|e| e.to_str()) == Some("app") {
            return Ok(path);
        }
    }
    Err(UpdateError::Install(
        "No .app bundle found in extracted zip".into(),
    ))
}

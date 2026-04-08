use std::path::PathBuf;

const REPO_OWNER: &str = "PoHsuanLai";
const REPO_NAME: &str = "rotero";
const ZIP_ASSET_SUFFIX: &str = "macos-arm64.zip";

#[derive(Debug, Clone, Default)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    ReadyToRestart,
    UpToDate,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateState {
    pub status: UpdateStatus,
    pub info: Option<UpdateInfo>,
    pub error: Option<String>,
    pub show_dialog: bool,
}

/// Check GitHub Releases for a newer version.
pub async fn check_for_update() -> Result<Option<UpdateInfo>, String> {
    let current = env!("CARGO_PKG_VERSION");
    let url = format!(
        "https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest"
    );

    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .get(&url)
        .header("User-Agent", "rotero-updater")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let tag = resp["tag_name"]
        .as_str()
        .ok_or("No tag_name in release")?;
    let latest_version = tag.trim_start_matches('v');

    if !version_gt(latest_version, current) {
        return Ok(None);
    }

    let release_notes = resp["body"].as_str().unwrap_or("").to_string();

    // Find the .zip asset for auto-update.
    let download_url = resp["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str().unwrap_or("");
                if name.ends_with(ZIP_ASSET_SUFFIX) {
                    a["browser_download_url"].as_str().map(String::from)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| format!("No {ZIP_ASSET_SUFFIX} asset found in release"))?;

    Ok(Some(UpdateInfo {
        current_version: current.to_string(),
        latest_version: latest_version.to_string(),
        release_notes,
        download_url,
    }))
}

/// Download the zip, extract the .app, replace the current bundle, and prepare for relaunch.
pub async fn apply_update(download_url: &str) -> Result<(), String> {
    let app_bundle = current_app_bundle()?;

    // Download the zip to a temp file.
    let client = reqwest::Client::new();
    let bytes = client
        .get(download_url)
        .header("User-Agent", "rotero-updater")
        .send()
        .await
        .map_err(|e| format!("Download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    let tmp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {e}"))?;
    let zip_path = tmp_dir.path().join("update.zip");
    std::fs::write(&zip_path, &bytes).map_err(|e| format!("Failed to write zip: {e}"))?;

    // Use ditto to extract (preserves macOS metadata, resource forks, code signatures).
    let extract_dir = tmp_dir.path().join("extracted");
    std::fs::create_dir_all(&extract_dir)
        .map_err(|e| format!("Failed to create extract dir: {e}"))?;

    let status = std::process::Command::new("ditto")
        .args(["-x", "-k"])
        .arg(&zip_path)
        .arg(&extract_dir)
        .status()
        .map_err(|e| format!("ditto failed: {e}"))?;
    if !status.success() {
        return Err("ditto extraction failed".to_string());
    }

    // Find the extracted .app.
    let new_app = find_app_in_dir(&extract_dir)?;

    // Replace the current .app bundle:
    // 1. Move old to trash (recoverable).
    // 2. Move new into place.
    let backup = app_bundle.with_extension("app.old");
    if backup.exists() {
        std::fs::remove_dir_all(&backup)
            .map_err(|e| format!("Failed to remove old backup: {e}"))?;
    }
    std::fs::rename(&app_bundle, &backup)
        .map_err(|e| format!("Failed to move current app: {e}"))?;

    if let Err(e) = std::fs::rename(&new_app, &app_bundle) {
        // Rollback: restore the old app.
        let _ = std::fs::rename(&backup, &app_bundle);
        return Err(format!("Failed to install new app: {e}"));
    }

    // Clean up backup.
    let _ = std::fs::remove_dir_all(&backup);

    // Keep tmp_dir alive until we're done (it auto-deletes on drop).
    // Leak it so it persists through relaunch.
    std::mem::forget(tmp_dir);

    tracing::info!("Update installed to {}", app_bundle.display());
    Ok(())
}

/// Get the path to the current .app bundle (e.g. /Applications/Rotero.app).
fn current_app_bundle() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("Can't find current exe: {e}"))?;
    // exe is like /path/to/Rotero.app/Contents/MacOS/rotero
    // Walk up to find the .app directory.
    let mut path = exe.as_path();
    loop {
        path = path
            .parent()
            .ok_or("Could not find .app bundle in exe path")?;
        if path.extension().and_then(|e| e.to_str()) == Some("app") {
            return Ok(path.to_path_buf());
        }
        if path.parent().is_none() {
            return Err("Could not find .app bundle in exe path".to_string());
        }
    }
}

/// Find a .app bundle inside a directory.
fn find_app_in_dir(dir: &std::path::Path) -> Result<PathBuf, String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("Failed to read extract dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.extension().and_then(|e| e.to_str()) == Some("app") {
            return Ok(path);
        }
    }
    Err("No .app bundle found in extracted zip".to_string())
}

/// Simple semver comparison: returns true if `a` is greater than `b`.
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.split('.');
        let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };
    parse(a) > parse(b)
}

#[cfg(target_os = "macos")]
use std::path::PathBuf;

const REPO_OWNER: &str = "PoHsuanLai";
const REPO_NAME: &str = "rotero";

/// Where to send someone whose platform has no downloadable build.
pub const RELEASES_PAGE: &str = "https://github.com/PoHsuanLai/rotero/releases/latest";

/// Suffix of the release asset that updates *this* build, or `None` where no
/// artifact is published.
///
/// Must track the artifact names produced by `.github/workflows/release.yml`.
/// A mismatch here means the updater silently reports "no asset" forever.
const fn update_asset_suffix() -> Option<&'static str> {
    std::cfg_select! {
        all(target_os = "macos", target_arch = "aarch64") => Some("macos-arm64.zip"),
        all(target_os = "windows", target_arch = "x86_64") => Some("windows-x64.zip"),
        all(target_os = "linux", target_arch = "x86_64") => Some("linux-x64.tar.gz"),
        _ => None,
    }
}

#[derive(Debug, Clone, Default)]
pub struct UpdateInfo {
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
    pub error: Option<UpdateError>,
    pub show_dialog: bool,
}

/// Why an update check or install failed.
///
/// Typed rather than a bare `String` so the dialog can say something useful —
/// the raw `reqwest`/`io` text was previously interpolated straight into the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Couldn't reach GitHub, or it returned something unusable.
    Network(String),
    /// A release exists, but nothing in it matches this platform.
    NoAssetForPlatform,
    /// This build can't replace itself in place (dev build, odd layout).
    NotInstalled(String),
    /// The download or swap itself failed.
    Install(String),
}

impl UpdateError {
    /// One-line summary for the dialog heading.
    pub fn headline(&self) -> &'static str {
        match self {
            Self::Network(_) => "Couldn't reach GitHub",
            Self::NoAssetForPlatform => "No download for this platform",
            Self::NotInstalled(_) => "Can't update this build",
            Self::Install(_) => "Couldn't install the update",
        }
    }

    /// What the user can actually do about it.
    pub fn guidance(&self) -> String {
        match self {
            Self::Network(detail) => {
                format!("Check your connection and try again.\n\n{detail}")
            }
            Self::NoAssetForPlatform => format!(
                "This release has no build for {} {}. Download it from the \
                 releases page instead.",
                std::env::consts::OS,
                std::env::consts::ARCH,
            ),
            Self::NotInstalled(detail) => format!(
                "Updating in place only works for an installed copy. \
                 Download the latest release manually.\n\n{detail}"
            ),
            Self::Install(detail) => {
                format!("Download the latest release manually.\n\n{detail}")
            }
        }
    }
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.headline(), self.guidance())
    }
}

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

/// Check GitHub Releases for a newer version.
pub async fn check_for_update() -> Result<Option<UpdateInfo>, UpdateError> {
    let current = env!("CARGO_PKG_VERSION");
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");

    let client = reqwest::Client::new();
    let resp: serde_json::Value = client
        .get(&url)
        .header("User-Agent", "rotero-updater")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?
        .json()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    // A rate-limit or error body has no tag_name, so report it as a network
    // problem rather than "no releases".
    let tag = resp["tag_name"]
        .as_str()
        .ok_or_else(|| UpdateError::Network("Unexpected response from GitHub".into()))?;
    let latest_version = tag.trim_start_matches('v');

    if !version_gt(latest_version, current) {
        return Ok(None);
    }

    let release_notes = resp["body"].as_str().unwrap_or("").to_string();

    // Only this platform's artifact can update this build.
    let suffix = update_asset_suffix().ok_or(UpdateError::NoAssetForPlatform)?;
    let download_url = resp["assets"]
        .as_array()
        .and_then(|assets| {
            assets.iter().find_map(|a| {
                let name = a["name"].as_str().unwrap_or("");
                if name.ends_with(suffix) {
                    a["browser_download_url"].as_str().map(String::from)
                } else {
                    None
                }
            })
        })
        .ok_or(UpdateError::NoAssetForPlatform)?;

    Ok(Some(UpdateInfo {
        latest_version: latest_version.to_string(),
        release_notes,
        download_url,
    }))
}

/// Download the new build and put it in place, ready for a restart.
///
/// The install step differs by platform because the artifacts differ in kind:
/// macOS ships a `.app` *directory* that has to be swapped whole, while
/// Windows and Linux ship a single executable that replaces itself.
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

    install_update(&bytes)
}

/// Replaces the running executable in place.
///
/// `self-replace` handles the part Windows makes hard: an image that is
/// currently executing can't simply be overwritten, so it has to be renamed
/// aside and cleaned up afterwards.
#[cfg(not(target_os = "macos"))]
fn install_update(bytes: &[u8]) -> Result<(), UpdateError> {
    let exe = std::env::current_exe()
        .map_err(|e| UpdateError::NotInstalled(format!("Can't locate this executable: {e}")))?;
    let exe_name = exe
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "rotero".into());

    let tmp_dir = tempfile::tempdir()
        .map_err(|e| UpdateError::Install(format!("Failed to create temp dir: {e}")))?;
    let staged = extract_executable(bytes, tmp_dir.path(), &exe_name)?;

    self_replace::self_replace(&staged)
        .map_err(|e| UpdateError::Install(format!("Failed to replace the executable: {e}")))?;

    tracing::info!("Update installed to {}", exe.display());
    Ok(())
}

/// Pulls the new executable out of the release archive into `dir`.
#[cfg(not(target_os = "macos"))]
fn extract_executable(
    bytes: &[u8],
    dir: &std::path::Path,
    exe_name: &str,
) -> Result<std::path::PathBuf, UpdateError> {
    let out = dir.join(exe_name);

    std::cfg_select! {
        windows => {
            let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
                .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
            for i in 0..archive.len() {
                let mut file = archive
                    .by_index(i)
                    .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
                let Some(name) = file.enclosed_name() else { continue };
                if name.file_name().is_some_and(|n| n == exe_name) {
                    let mut sink = std::fs::File::create(&out).map_err(|e| {
                        UpdateError::Install(format!("Failed to stage the update: {e}"))
                    })?;
                    std::io::copy(&mut file, &mut sink).map_err(|e| {
                        UpdateError::Install(format!("Failed to stage the update: {e}"))
                    })?;
                    return Ok(out);
                }
            }
        }
        _ => {
            let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
            let entries = archive
                .entries()
                .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
            for entry in entries {
                let mut entry = entry
                    .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?;
                let path = entry
                    .path()
                    .map_err(|e| UpdateError::Install(format!("Bad update archive: {e}")))?
                    .into_owned();
                if path.file_name().is_some_and(|n| n == exe_name) {
                    entry.unpack(&out).map_err(|e| {
                        UpdateError::Install(format!("Failed to stage the update: {e}"))
                    })?;
                    return Ok(out);
                }
            }
        }
    }

    Err(UpdateError::Install(format!(
        "No `{exe_name}` inside the downloaded release"
    )))
}

/// Extract the `.app` and swap it for the running bundle.
#[cfg(target_os = "macos")]
fn install_update(bytes: &[u8]) -> Result<(), UpdateError> {
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

    // Find the extracted .app.
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
        // Rollback: restore the old app.
        let _ = std::fs::rename(&backup, &app_bundle);
        return Err(UpdateError::Install(format!(
            "Failed to install new app: {e}"
        )));
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
#[cfg(target_os = "macos")]
fn current_app_bundle() -> Result<PathBuf, UpdateError> {
    // A dev build (`cargo run` / `dx serve`) has no bundle, so this is the
    // usual reason an update can't be applied in place.
    let not_bundled =
        || UpdateError::NotInstalled("This copy isn't running from a Rotero.app bundle.".into());

    let exe = std::env::current_exe()
        .map_err(|e| UpdateError::NotInstalled(format!("Can't find current exe: {e}")))?;
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

/// Find a .app bundle inside a directory.
#[cfg(target_os = "macos")]
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

/// Whether release `a` is newer than release `b`.
///
/// Compares `major.minor.patch` numerically. A pre-release suffix
/// (`0.3.0-rc1`) is treated as *older* than the release it precedes, so an
/// rc tag is never offered as an upgrade over the final version — previously
/// the suffix parsed to `0` and `0.3.0-rc1` compared equal to `0.3.0`.
fn version_gt(a: &str, b: &str) -> bool {
    /// `(major, minor, patch, is_release)` — the flag orders `0.3.0` above
    /// `0.3.0-rc1` while leaving the numeric comparison untouched.
    fn parse(s: &str) -> (u32, u32, u32, bool) {
        let core = s.split(['-', '+']).next().unwrap_or(s);
        let is_release = core.len() == s.len();
        let mut parts = core.split('.');
        let mut next = || {
            parts
                .next()
                .and_then(|p| p.trim().parse::<u32>().ok())
                .unwrap_or(0)
        };
        (next(), next(), next(), is_release)
    }
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_versions_compare_greater() {
        assert!(version_gt("0.2.1", "0.2.0"));
        assert!(version_gt("0.3.0", "0.2.9"));
        assert!(version_gt("1.0.0", "0.9.9"));
        assert!(version_gt("0.2.10", "0.2.9"), "numeric, not lexicographic");
    }

    #[test]
    fn same_or_older_versions_do_not() {
        assert!(!version_gt("0.2.0", "0.2.0"));
        assert!(!version_gt("0.2.0", "0.2.1"));
        assert!(!version_gt("0.9.9", "1.0.0"));
    }

    /// The shipped v0.1.6 -> v0.2.0 upgrade path.
    #[test]
    fn minor_rollover_is_an_upgrade() {
        assert!(version_gt("0.2.0", "0.1.6"));
    }

    #[test]
    fn prereleases_rank_below_their_release() {
        assert!(version_gt("0.3.0", "0.3.0-rc1"));
        assert!(!version_gt("0.3.0-rc1", "0.3.0"));
        // ...but a prerelease of a higher version still beats a lower release.
        assert!(version_gt("0.3.0-rc1", "0.2.9"));
    }

    #[test]
    fn short_and_padded_versions_are_equivalent() {
        assert!(!version_gt("0.3", "0.3.0"));
        assert!(version_gt("0.3", "0.2.9"));
    }

    /// Every published asset name must be matchable, or the updater reports
    /// "no asset" forever on that platform.
    #[test]
    fn this_platform_has_a_known_asset_suffix() {
        let suffix = update_asset_suffix();
        if cfg!(any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "windows", target_arch = "x86_64"),
            all(target_os = "linux", target_arch = "x86_64"),
        )) {
            assert!(suffix.is_some(), "expected an asset for this target");
        }
        // A platform with no published build must say so rather than guess.
        if let Some(s) = suffix {
            assert!(s.ends_with(".zip") || s.ends_with(".tar.gz"));
        }
    }
}

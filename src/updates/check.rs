use super::types::{UpdateError, UpdateInfo};

const REPO_OWNER: &str = "PoHsuanLai";
const REPO_NAME: &str = "rotero";

/// Where to send someone whose platform has no downloadable build.
pub const RELEASES_PAGE: &str = "https://github.com/PoHsuanLai/rotero/releases/latest";

/// Suffix of the release asset that updates *this* build, or `None` where no
/// artifact is published.
///
/// Must track the artifact names produced by `.github/workflows/release.yml`.
/// A mismatch here means the updater silently reports "no asset" forever.
pub(super) const fn update_asset_suffix() -> Option<&'static str> {
    std::cfg_select! {
        all(target_os = "macos", target_arch = "aarch64") => Some("macos-arm64.zip"),
        all(target_os = "windows", target_arch = "x86_64") => Some("windows-x64.zip"),
        all(target_os = "linux", target_arch = "x86_64") => Some("linux-x64.tar.gz"),
        _ => None,
    }
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

/// Whether release `a` is newer than release `b`.
///
/// Compares `major.minor.patch` numerically. A pre-release suffix
/// (`0.3.0-rc1`) is treated as *older* than the release it precedes, so an
/// rc tag is never offered as an upgrade over the final version — previously
/// the suffix parsed to `0` and `0.3.0-rc1` compared equal to `0.3.0`.
pub(super) fn version_gt(a: &str, b: &str) -> bool {
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

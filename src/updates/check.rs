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
    let latest = fetch_latest_release().await?;
    let latest_version = latest.tag.trim_start_matches('v');

    if !version_gt(latest_version, current) {
        return Ok(None);
    }

    let suffix = update_asset_suffix().ok_or(UpdateError::NoAssetForPlatform)?;
    let download_url = latest.download_url_for(suffix);

    Ok(Some(UpdateInfo {
        latest_version: latest_version.to_string(),
        release_notes: latest.notes,
        download_url,
    }))
}

struct LatestRelease {
    tag: String,
    notes: String,
    /// `(filename, browser_download_url)` from the API, if we got that far.
    assets: Vec<(String, String)>,
}

impl LatestRelease {
    fn download_url_for(&self, suffix: &str) -> String {
        self.assets
            .iter()
            .find(|(name, _)| name.ends_with(suffix))
            .map(|(_, url)| url.clone())
            .unwrap_or_else(|| download_url_for_tag(&self.tag, suffix))
    }
}

/// REST API first (notes + asset URLs). Unauthenticated GitHub allows 60
/// requests per hour per IP, which a shared NAT burns quickly — the app used
/// to treat the 403 JSON as "Unexpected response from GitHub". On any API
/// failure, fall back to the HTML latest-release redirect, which is not
/// rate-limited the same way.
async fn fetch_latest_release() -> Result<LatestRelease, UpdateError> {
    match fetch_latest_via_api().await {
        Ok(latest) => Ok(latest),
        Err(api_err) => match fetch_latest_via_web().await {
            Ok(latest) => Ok(latest),
            Err(_) => Err(api_err),
        },
    }
}

async fn fetch_latest_via_api() -> Result<LatestRelease, UpdateError> {
    let url = format!("https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent("rotero-updater")
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = resp.status();
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;

    if !status.is_success() {
        return Err(UpdateError::Network(api_error_message(status, &body)));
    }

    let tag = body["tag_name"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| UpdateError::Network("Unexpected response from GitHub".into()))?
        .to_string();

    let notes = body["body"].as_str().unwrap_or("").to_string();
    let mut assets = Vec::new();
    if let Some(list) = body["assets"].as_array() {
        for a in list {
            if let (Some(name), Some(url)) =
                (a["name"].as_str(), a["browser_download_url"].as_str())
            {
                assets.push((name.to_string(), url.to_string()));
            }
        }
    }
    Ok(LatestRelease { tag, notes, assets })
}

/// `GET /releases/latest` 302s to `/releases/tag/vX.Y.Z`. That hop does not
/// count against the REST rate limit.
async fn fetch_latest_via_web() -> Result<LatestRelease, UpdateError> {
    let url = format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent("rotero-updater")
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let location = resp
        .headers()
        .get(reqwest::header::LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| UpdateError::Network("Unexpected response from GitHub".into()))?;
    let tag = tag_from_latest_location(location)
        .ok_or_else(|| UpdateError::Network("Unexpected response from GitHub".into()))?;
    Ok(LatestRelease {
        tag,
        notes: String::new(),
        assets: Vec::new(),
    })
}

fn api_error_message(status: reqwest::StatusCode, body: &serde_json::Value) -> String {
    let msg = body["message"].as_str().unwrap_or("").trim();
    if status.as_u16() == 403 || status.as_u16() == 429 {
        if msg.to_ascii_lowercase().contains("rate limit") {
            return "GitHub rate-limited this network. Try again in a few minutes.".into();
        }
        if !msg.is_empty() {
            return msg.to_string();
        }
        return format!("GitHub returned {status}");
    }
    if !msg.is_empty() {
        return format!("GitHub returned {status}: {msg}");
    }
    format!("GitHub returned {status}")
}

/// `https://github.com/owner/repo/releases/tag/v0.2.7` → `v0.2.7`.
fn tag_from_latest_location(location: &str) -> Option<String> {
    const MARKER: &str = "/releases/tag/";
    let rest = location.split(MARKER).nth(1)?;
    let tag = rest.split(['?', '#', '/']).next()?.trim();
    if tag.is_empty() {
        None
    } else {
        Some(tag.to_string())
    }
}

fn download_url_for_tag(tag: &str, suffix: &str) -> String {
    format!(
        "https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/{tag}/Rotero-{tag}-{suffix}"
    )
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

    #[test]
    fn tag_from_absolute_and_relative_locations() {
        assert_eq!(
            tag_from_latest_location("https://github.com/PoHsuanLai/rotero/releases/tag/v0.2.7"),
            Some("v0.2.7".into())
        );
        assert_eq!(
            tag_from_latest_location("/PoHsuanLai/rotero/releases/tag/v0.2.7"),
            Some("v0.2.7".into())
        );
        assert_eq!(
            tag_from_latest_location(
                "https://github.com/PoHsuanLai/rotero/releases/tag/v0.2.7?foo=1"
            ),
            Some("v0.2.7".into())
        );
        assert_eq!(
            tag_from_latest_location("https://github.com/PoHsuanLai/rotero/releases/latest"),
            None
        );
    }

    #[test]
    fn constructed_download_url_matches_release_assets() {
        assert_eq!(
            download_url_for_tag("v0.2.7", "linux-x64.tar.gz"),
            "https://github.com/PoHsuanLai/rotero/releases/download/v0.2.7/Rotero-v0.2.7-linux-x64.tar.gz"
        );
        assert_eq!(
            download_url_for_tag("v0.2.7", "macos-arm64.zip"),
            "https://github.com/PoHsuanLai/rotero/releases/download/v0.2.7/Rotero-v0.2.7-macos-arm64.zip"
        );
    }

    #[test]
    fn rate_limit_body_is_a_clear_network_error() {
        let body = serde_json::json!({
            "message": "API rate limit exceeded for 1.2.3.4."
        });
        let msg = api_error_message(reqwest::StatusCode::FORBIDDEN, &body);
        assert!(msg.to_ascii_lowercase().contains("rate-limited"));
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

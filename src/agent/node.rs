use std::path::{Path, PathBuf};

const NODE_VERSION: &str = "v22.16.0";

/// Find node binary: check PATH first, then local cache, then download.
pub(crate) fn find_or_install_node() -> Result<PathBuf, String> {
    // 1. Check PATH
    if let Ok(path) = which::which("node") {
        return Ok(path);
    }

    // 2. Check local cache
    let node_dir = node_cache_dir();
    let node_bin = node_binary_path(&node_dir);
    if node_bin.exists() {
        return Ok(node_bin);
    }

    // 3. Download
    tracing::info!("Node.js not found, downloading {NODE_VERSION}...");
    download_node(&node_dir)?;

    if node_bin.exists() {
        Ok(node_bin)
    } else {
        Err("Downloaded Node.js but binary not found".into())
    }
}

pub(crate) fn find_npm() -> Result<PathBuf, String> {
    if let Ok(path) = which::which("npm") {
        return Ok(path);
    }
    let node_dir = node_cache_dir();
    let npm_bin = npm_binary_path(&node_dir);
    if npm_bin.exists() {
        return Ok(npm_bin);
    }
    Err("npm not found".into())
}

fn node_cache_dir() -> PathBuf {
    #[cfg(feature = "desktop")]
    let base = directories::BaseDirs::new()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(feature = "desktop"))]
    let base = PathBuf::from(".");
    base.join("com.rotero.Rotero").join("node")
}

/// Windows lays the distribution out flat; Unix puts executables under `bin/`.
fn node_binary_path(node_dir: &Path) -> PathBuf {
    std::cfg_select! {
        windows => node_dir.join("node.exe"),
        _ => node_dir.join("bin").join("node"),
    }
}

fn npm_binary_path(node_dir: &Path) -> PathBuf {
    std::cfg_select! {
        windows => node_dir.join("npm.cmd"),
        _ => node_dir.join("bin").join("npm"),
    }
}

/// Downloads and unpacks the Node.js distribution into `node_dir`.
///
/// Runs on the agent's own thread, so it uses the blocking HTTP client and
/// decompresses in-process — no `curl`/`tar` on PATH required.
fn download_node(node_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(node_dir)
        .map_err(|e| format!("Failed to create node cache dir: {e}"))?;

    let (os, arch, ext) = node_platform()?;
    let archive_name = format!("node-{NODE_VERSION}-{os}-{arch}.{ext}");
    let url = format!("https://nodejs.org/dist/{NODE_VERSION}/{archive_name}");

    tracing::info!("Downloading Node.js from {url}");

    let resp = reqwest::blocking::Client::builder()
        .user_agent("rotero")
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?
        .get(&url)
        .send()
        .map_err(|e| format!("Failed to download Node.js: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Failed to download Node.js: HTTP {}",
            resp.status()
        ));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| format!("Failed to read Node.js download: {e}"))?;

    if ext == "zip" {
        unpack_zip(&bytes, node_dir)?;
    } else {
        unpack_tar_gz(&bytes, node_dir)?;
    }

    tracing::info!("Node.js {NODE_VERSION} installed to {}", node_dir.display());
    Ok(())
}

/// Strips the leading `node-{version}-{os}-{arch}/` directory every Node.js
/// archive wraps its contents in, so entries land directly in `node_dir`.
///
/// Returns `None` for the wrapper directory itself and for any entry that
/// escapes it (`..`, absolute paths) — a zip-slip guard, since these archives
/// are unpacked into a user directory.
fn strip_archive_prefix(path: &Path) -> Option<PathBuf> {
    let rest: PathBuf = path.components().skip(1).collect();
    if rest.as_os_str().is_empty() {
        return None;
    }
    let safe = rest.components().all(|c| {
        matches!(
            c,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    });
    safe.then_some(rest)
}

fn unpack_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read Node.js tarball: {e}"))?;

    for entry in entries {
        let mut entry = entry.map_err(|e| format!("Failed to read tarball entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("Bad path in tarball: {e}"))?
            .into_owned();
        let Some(rel) = strip_archive_prefix(&path) else {
            continue;
        };
        entry
            .unpack(dest.join(rel))
            .map_err(|e| format!("Failed to unpack {}: {e}", path.display()))?;
    }
    Ok(())
}

fn unpack_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("Failed to read Node.js zip: {e}"))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {e}"))?;
        // `enclosed_name` already rejects paths that escape the archive root.
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let Some(rel) = strip_archive_prefix(&name) else {
            continue;
        };
        let out = dest.join(rel);

        if file.is_dir() {
            std::fs::create_dir_all(&out)
                .map_err(|e| format!("Failed to create {}: {e}", out.display()))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        let mut sink = std::fs::File::create(&out)
            .map_err(|e| format!("Failed to create {}: {e}", out.display()))?;
        std::io::copy(&mut file, &mut sink)
            .map_err(|e| format!("Failed to write {}: {e}", out.display()))?;

        #[cfg(unix)]
        if let Some(mode) = file.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
        }
    }
    Ok(())
}

/// The `(os, arch, ext)` triple naming this platform's Node.js distribution.
///
/// `.tar.gz` rather than `.tar.xz` on Unix: gzip is decompressed by `flate2`,
/// already in the dependency tree, whereas xz would pull in native liblzma.
fn node_platform() -> Result<(&'static str, &'static str, &'static str), String> {
    // Don't guess: an unrecognised OS or architecture previously fell through
    // to the Windows x64 build.
    let os = std::cfg_select! {
        target_os = "macos" => "darwin",
        target_os = "linux" => "linux",
        target_os = "windows" => "win",
        _ => return Err(format!(
            "No Node.js build for this platform ({}); install Node.js and put \
             it on PATH",
            std::env::consts::OS
        )),
    };

    let arch = std::cfg_select! {
        target_arch = "aarch64" => "arm64",
        target_arch = "x86_64" => "x64",
        _ => return Err(format!(
            "No Node.js build for this architecture ({}); install Node.js and \
             put it on PATH",
            std::env::consts::ARCH
        )),
    };

    let ext = std::cfg_select! {
        windows => "zip",
        _ => "tar.gz",
    };

    Ok((os, arch, ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_the_wrapping_directory() {
        assert_eq!(
            strip_archive_prefix(Path::new("node-v22.16.0-win-x64/node.exe")),
            Some(PathBuf::from("node.exe"))
        );
        assert_eq!(
            strip_archive_prefix(Path::new("node-v22.16.0-darwin-arm64/bin/node")),
            Some(PathBuf::from("bin/node"))
        );
    }

    #[test]
    fn drops_the_wrapper_directory_entry_itself() {
        assert_eq!(
            strip_archive_prefix(Path::new("node-v22.16.0-win-x64")),
            None
        );
        assert_eq!(
            strip_archive_prefix(Path::new("node-v22.16.0-win-x64/")),
            None
        );
    }

    /// The archive is unpacked into a user directory, so an entry that climbs
    /// out of it must be refused rather than written.
    #[test]
    fn rejects_paths_escaping_the_archive_root() {
        assert_eq!(
            strip_archive_prefix(Path::new("node-v22.16.0-win-x64/../../evil")),
            None
        );
        assert_eq!(
            strip_archive_prefix(Path::new("node-v22.16.0-win-x64/a/../../../evil")),
            None
        );
    }

    /// The binary paths the installer probes must match what the strip leaves
    /// behind, or a successful download still reports "binary not found".
    #[test]
    fn binary_paths_line_up_with_extracted_layout() {
        let dir = Path::new("/cache/node");
        if cfg!(windows) {
            assert_eq!(node_binary_path(dir), dir.join("node.exe"));
            assert_eq!(npm_binary_path(dir), dir.join("npm.cmd"));
        } else {
            assert_eq!(node_binary_path(dir), dir.join("bin").join("node"));
            assert_eq!(npm_binary_path(dir), dir.join("bin").join("npm"));
        }
    }

    #[test]
    fn platform_triple_is_known_for_this_target() {
        let (os, arch, ext) = node_platform().expect("host platform should be supported");
        assert!(matches!(os, "darwin" | "linux" | "win"));
        assert!(matches!(arch, "arm64" | "x64"));
        assert_eq!(ext, if cfg!(windows) { "zip" } else { "tar.gz" });
    }
}

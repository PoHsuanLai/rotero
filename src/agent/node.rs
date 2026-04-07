use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) const NODE_VERSION: &str = "v22.16.0";

pub(crate) fn find_or_install_node() -> Result<PathBuf, String> {
    if let Ok(path) = which::which("node") {
        return Ok(path);
    }

    let node_dir = node_cache_dir();
    let node_bin = node_binary_path(&node_dir);
    if node_bin.exists() {
        return Ok(node_bin);
    }

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
    Err("npm not found. Install Node.js or use the AI Agent settings to auto-download.".into())
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

fn node_binary_path(node_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        node_dir.join("node.exe")
    } else {
        node_dir.join("bin").join("node")
    }
}

fn npm_binary_path(node_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        node_dir.join("npm.cmd")
    } else {
        node_dir.join("bin").join("npm")
    }
}

fn download_node(node_dir: &PathBuf) -> Result<(), String> {
    std::fs::create_dir_all(node_dir)
        .map_err(|e| format!("Failed to create node cache dir: {e}"))?;

    let (os, arch, ext) = node_platform();
    let filename = format!("node-{NODE_VERSION}-{os}-{arch}");
    let archive_name = format!("{filename}.{ext}");
    let url = format!("https://nodejs.org/dist/{NODE_VERSION}/{archive_name}");

    tracing::info!("Downloading Node.js from {url}");

    let tmp_archive = node_dir.join(&archive_name);
    let output = Command::new("curl")
        .args(["-fsSL", "-o", &tmp_archive.to_string_lossy(), &url])
        .output()
        .map_err(|e| format!("Failed to download Node.js: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Download failed: {stderr}"));
    }

    if ext == "zip" {
        let output = Command::new("tar")
            .args([
                "-xf",
                &tmp_archive.to_string_lossy(),
                "-C",
                &node_dir.to_string_lossy(),
            ])
            .output()
            .map_err(|e| format!("Failed to extract: {e}"))?;
        if !output.status.success() {
            return Err("Failed to extract Node.js archive".into());
        }
    } else {
        let output = Command::new("tar")
            .args([
                "-xf",
                &tmp_archive.to_string_lossy(),
                "-C",
                &node_dir.to_string_lossy(),
                "--strip-components=1",
            ])
            .output()
            .map_err(|e| format!("Failed to extract: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Failed to extract Node.js: {stderr}"));
        }
    }

    let _ = std::fs::remove_file(&tmp_archive);

    tracing::info!("Node.js {NODE_VERSION} installed to {}", node_dir.display());
    Ok(())
}

fn node_platform() -> (&'static str, &'static str, &'static str) {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "win"
    } else {
        "linux" // fallback
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else if cfg!(target_arch = "arm") {
        "armv7l"
    } else {
        "x64" // fallback
    };

    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.xz"
    };

    (os, arch, ext)
}

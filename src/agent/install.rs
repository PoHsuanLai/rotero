use std::path::PathBuf;
use std::process::Command;

use super::node::find_npm;
use super::types::AgentProvider;

pub(crate) fn agents_cache_dir() -> PathBuf {
    let base = directories::BaseDirs::new()
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("com.rotero.Rotero").join("agents")
}

pub(crate) fn ensure_agent_installed(provider: &AgentProvider) -> Result<PathBuf, String> {
    let cache = agents_cache_dir();
    let pkg_dir = cache.join(provider.id);
    let pkg_root = pkg_dir.join("node_modules").join(provider.npm_package);
    let pkg_json_path = pkg_root.join("package.json");

    if pkg_json_path.exists() {
        return resolve_bin_entry(&pkg_root);
    }

    std::fs::create_dir_all(&pkg_dir)
        .map_err(|e| format!("Failed to create agent cache dir: {e}"))?;

    tracing::info!("Installing {} (first time setup)...", provider.npm_package);

    let npm_bin = find_npm()?;
    let output = Command::new(&npm_bin)
        .args([
            "install",
            "--prefix",
            &pkg_dir.to_string_lossy(),
            provider.npm_package,
        ])
        .output()
        .map_err(|e| format!("Failed to run npm install: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("npm install failed: {stderr}"));
    }

    resolve_bin_entry(&pkg_root)
}

pub(crate) fn resolve_bin_entry(pkg_root: &PathBuf) -> Result<PathBuf, String> {
    let pkg_json = pkg_root.join("package.json");
    let content =
        std::fs::read_to_string(&pkg_json).map_err(|e| format!("Can't read package.json: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid package.json: {e}"))?;

    let bin_path = match v.get("bin") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Object(obj)) => obj
            .values()
            .next()
            .and_then(|v| v.as_str())
            .ok_or("No bin entries in package.json")?
            .to_string(),
        _ => return Err("No bin field in package.json".into()),
    };

    let entry = pkg_root.join(&bin_path);
    if entry.exists() {
        Ok(entry)
    } else {
        Err(format!("Entry point not found: {}", entry.display()))
    }
}

pub(crate) fn find_mcp_binary() -> Option<PathBuf> {
    // 1. Next to the running binary (production)
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("rotero-mcp");
        if sibling.exists() {
            return Some(sibling);
        }
    }

    // 2. In the workspace target dir (development — handles worktrees)
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for dir in [&manifest_dir, &manifest_dir.join(".."), &manifest_dir.join("../..")]  {
        for profile in ["release", "debug"] {
            let candidate = dir.join("target").join(profile).join("rotero-mcp");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 3. In PATH
    which::which("rotero-mcp").ok()
}

pub(crate) fn find_pdfium_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
        return Some(PathBuf::from(p));
    }
    let lib = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lib");
    if lib.exists() {
        return Some(lib);
    }
    None
}

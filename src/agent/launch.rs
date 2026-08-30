use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use agent_client_protocol::AcpAgentConfig;

use super::install::agents_cache_dir;
use super::node::{find_npm, find_or_install_node};
use super::registry::{BinaryDist, NpxDist, RegistryAgent, UvxDist};

#[derive(Debug, Clone, PartialEq)]
pub struct LaunchSpec {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl LaunchSpec {
    pub fn into_agent_config(self) -> AcpAgentConfig {
        let mut config = AcpAgentConfig::new(&self.command).args(self.args);
        for (key, value) in self.env {
            config = config.env(key, value);
        }
        config
    }
}

pub fn platform_triple() -> Result<String, String> {
    let os = std::cfg_select! {
        target_os = "macos" => "darwin",
        target_os = "linux" => "linux",
        target_os = "windows" => "windows",
        _ => return Err(format!(
            "No ACP agent builds for this platform ({})",
            std::env::consts::OS
        )),
    };
    let arch = std::cfg_select! {
        target_arch = "aarch64" => "aarch64",
        target_arch = "x86_64" => "x86_64",
        _ => return Err(format!(
            "No ACP agent builds for this architecture ({})",
            std::env::consts::ARCH
        )),
    };
    Ok(format!("{os}-{arch}"))
}

/// Resolve how to spawn `agent` for this machine.
///
/// Prefer a binary already on PATH, then a registry binary for this platform,
/// then npx, then uvx.
pub fn resolve_launch(agent: &RegistryAgent) -> Result<LaunchSpec, String> {
    let platform = platform_triple()?;
    if let Some(binary) = agent
        .distribution
        .binary
        .as_ref()
        .and_then(|map| map.get(&platform))
    {
        if let Some(spec) = probe_local_binary(binary) {
            return Ok(spec.with_env(binary.env.clone()));
        }
        return install_binary(agent, binary);
    }

    if let Some(npx) = &agent.distribution.npx {
        if let Some(spec) = probe_npx_bin_on_path(npx) {
            return Ok(spec.with_env(npx.env.clone()));
        }
        return npx_launch(npx);
    }

    if let Some(uvx) = &agent.distribution.uvx {
        return uvx_launch(uvx);
    }

    Err(format!(
        "{} has no launch method for this platform",
        agent.name
    ))
}

impl LaunchSpec {
    fn with_env(mut self, extra: HashMap<String, String>) -> Self {
        self.env.extend(extra);
        self
    }
}

fn cmd_basename(cmd: &str) -> &str {
    Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd)
        .trim_start_matches("./")
}

fn probe_local_binary(binary: &BinaryDist) -> Option<LaunchSpec> {
    let name = cmd_basename(&binary.cmd);
    let path = which::which(name).ok()?;
    Some(LaunchSpec {
        command: path,
        args: binary.args.clone(),
        env: Vec::new(),
    })
}

fn npx_bin_name(package: &str) -> String {
    // `@scope/name@1.2.3` — the version is the last `@` followed by a digit.
    let name = match package.rsplit_once('@') {
        Some((prefix, ver)) if ver.starts_with(|c: char| c.is_ascii_digit()) => prefix,
        _ => package,
    };
    name.rsplit('/').next().unwrap_or(name).to_string()
}

fn probe_npx_bin_on_path(npx: &NpxDist) -> Option<LaunchSpec> {
    let name = npx_bin_name(&npx.package);
    let path = which::which(&name).ok()?;
    Some(LaunchSpec {
        command: path,
        args: npx.args.clone(),
        env: Vec::new(),
    })
}

fn npx_launch(npx: &NpxDist) -> Result<LaunchSpec, String> {
    let node = find_or_install_node()?;
    let npm = find_npm()?;
    let mut env = npx.env.clone().into_iter().collect::<Vec<_>>();
    if let Some(node_dir) = node.parent() {
        let path = std::env::var("PATH").unwrap_or_default();
        let sep = if cfg!(windows) { ';' } else { ':' };
        let prepended = if path.is_empty() {
            node_dir.display().to_string()
        } else {
            format!("{}{sep}{path}", node_dir.display())
        };
        env.push(("PATH".into(), prepended));
    }

    let mut args = vec![
        "exec".into(),
        "--yes".into(),
        "--".into(),
        npx.package.clone(),
    ];
    args.extend(npx.args.iter().cloned());

    if super::helpers::is_batch_file(&npm) {
        let mut wrapped = vec!["/C".into(), npm.to_string_lossy().into_owned()];
        wrapped.extend(args);
        Ok(LaunchSpec {
            command: PathBuf::from("cmd"),
            args: wrapped,
            env,
        })
    } else {
        Ok(LaunchSpec {
            command: npm,
            args,
            env,
        })
    }
}

fn uvx_launch(uvx: &UvxDist) -> Result<LaunchSpec, String> {
    let uvx_bin = which::which("uvx").map_err(|_| {
        format!(
            "This agent is distributed with uvx ({}) but `uv` is not on PATH",
            uvx.package
        )
    })?;
    let mut args = vec![uvx.package.clone()];
    args.extend(uvx.args.iter().cloned());
    Ok(LaunchSpec {
        command: uvx_bin,
        args,
        env: uvx.env.clone().into_iter().collect(),
    })
}

fn install_binary(agent: &RegistryAgent, binary: &BinaryDist) -> Result<LaunchSpec, String> {
    let dest = agents_cache_dir()
        .join(&agent.id)
        .join(if agent.version.is_empty() {
            "latest"
        } else {
            &agent.version
        });
    let cmd_rel = binary.cmd.trim_start_matches("./");
    let entry = dest.join(cmd_rel);
    if entry.exists() {
        return Ok(LaunchSpec {
            command: entry,
            args: binary.args.clone(),
            env: binary.env.clone().into_iter().collect(),
        });
    }

    tracing::info!("Downloading {} ({})…", agent.name, agent.version);
    download_and_extract(&binary.archive, binary.sha256.as_deref(), &dest)?;

    if !entry.exists() {
        return Err(format!(
            "{} archive did not contain {}",
            agent.name,
            entry.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&entry)
            .map_err(|e| format!("Failed to read {}: {e}", entry.display()))?
            .permissions();
        perms.set_mode(perms.mode() | 0o755);
        let _ = std::fs::set_permissions(&entry, perms);
    }

    Ok(LaunchSpec {
        command: entry,
        args: binary.args.clone(),
        env: binary.env.clone().into_iter().collect(),
    })
}

const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

fn download_and_extract(url: &str, sha256: Option<&str>, dest: &Path) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("rotero")
        .timeout(DOWNLOAD_TIMEOUT)
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("Failed to download agent: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Failed to download agent: HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .map_err(|e| format!("Failed to read agent download: {e}"))?;

    if let Some(expected) = sha256 {
        let actual = sha256_hex(&bytes);
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(format!(
                "Agent download failed verification (expected {expected}, got {actual})"
            ));
        }
    }

    let parent = dest
        .parent()
        .ok_or_else(|| "Invalid agent cache path".to_string())?;
    std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create agent cache: {e}"))?;
    let staging = parent.join(format!(".staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| format!("Failed to create staging dir: {e}"))?;

    let unpacked = if url.ends_with(".zip") {
        unpack_zip(&bytes, &staging)
    } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        unpack_tar_gz(&bytes, &staging)
    } else if url.ends_with(".tar.bz2") || url.ends_with(".tbz2") {
        unpack_tar_bz2(&bytes, &staging)
    } else {
        // Raw binary: write as the last path component of dest's expected cmd
        // is not known here; write as the URL leaf.
        let leaf = url.rsplit('/').next().unwrap_or("agent");
        let out = staging.join(leaf);
        std::fs::write(&out, &bytes).map_err(|e| format!("Failed to write binary: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o755));
        }
        Ok(())
    };
    if let Err(e) = unpacked {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }

    let previous = dest.with_extension("previous");
    let _ = std::fs::remove_dir_all(&previous);
    let had_previous = dest.exists() && std::fs::rename(dest, &previous).is_ok();
    if let Err(e) = std::fs::rename(&staging, dest) {
        let _ = std::fs::remove_dir_all(&staging);
        if had_previous {
            let _ = std::fs::rename(&previous, dest);
        }
        return Err(format!("Failed to install agent: {e}"));
    }
    if had_previous {
        let _ = std::fs::remove_dir_all(&previous);
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn safe_rel(path: &Path) -> Option<PathBuf> {
    let safe = path.components().all(|c| {
        matches!(
            c,
            std::path::Component::Normal(_) | std::path::Component::CurDir
        )
    });
    safe.then(|| path.to_path_buf())
}

fn unpack_tar_gz(bytes: &[u8], dest: &Path) -> Result<(), String> {
    unpack_tar(
        flate2::read::GzDecoder::new(std::io::Cursor::new(bytes)),
        dest,
    )
}

fn unpack_tar_bz2(bytes: &[u8], dest: &Path) -> Result<(), String> {
    unpack_tar(
        bzip2::read::BzDecoder::new(std::io::Cursor::new(bytes)),
        dest,
    )
}

fn unpack_tar<R: Read>(reader: R, dest: &Path) -> Result<(), String> {
    let mut archive = tar::Archive::new(reader);
    let entries = archive
        .entries()
        .map_err(|e| format!("Failed to read agent tarball: {e}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| format!("Failed to read tarball entry: {e}"))?;
        let path = entry
            .path()
            .map_err(|e| format!("Bad path in tarball: {e}"))?
            .into_owned();
        let Some(rel) = safe_rel(&path) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        entry
            .unpack(dest.join(rel))
            .map_err(|e| format!("Failed to unpack {}: {e}", path.display()))?;
    }
    Ok(())
}

fn unpack_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("Failed to read agent zip: {e}"))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {e}"))?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let Some(rel) = safe_rel(&name) else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::registry::Registry;

    fn fixture() -> Registry {
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/acp_registry.json"
        ));
        serde_json::from_str(json).unwrap()
    }

    fn agent(id: &str) -> RegistryAgent {
        fixture().agents.into_iter().find(|a| a.id == id).expect(id)
    }

    #[test]
    fn platform_triple_is_known() {
        let triple = platform_triple().expect("host platform should be supported");
        assert!(
            triple.contains("darwin") || triple.contains("linux") || triple.contains("windows")
        );
    }

    #[test]
    fn npx_spec_uses_npm_exec_and_package_args() {
        let grok = agent("grok-build");
        let npx = grok.distribution.npx.unwrap();
        assert_eq!(npx.package, "@xai-official/grok@1.0.13");
        assert_eq!(npx.args, vec!["agent", "stdio"]);
        assert_eq!(npx_bin_name(&npx.package), "grok");
    }

    #[test]
    fn npx_bin_name_strips_scope_and_version() {
        assert_eq!(
            npx_bin_name("@agentclientprotocol/claude-agent-acp@0.70.0"),
            "claude-agent-acp"
        );
        assert_eq!(npx_bin_name("cline@3.0.60"), "cline");
    }

    #[test]
    fn binary_cmd_basename_strips_dot_slash() {
        assert_eq!(cmd_basename("./crow-cli"), "crow-cli");
        assert_eq!(cmd_basename("./crow-cli.exe"), "crow-cli.exe");
        assert_eq!(cmd_basename("amp-acp"), "amp-acp");
    }

    #[test]
    fn uvx_agent_is_detected() {
        let fast = agent("fast-agent");
        let uvx = fast.distribution.uvx.unwrap();
        assert_eq!(uvx.package, "fast-agent-acp==0.10.1");
        assert_eq!(uvx.args, vec!["-x"]);
    }

    #[test]
    fn rejects_paths_escaping_the_archive() {
        assert_eq!(safe_rel(Path::new("../../evil")), None);
        assert_eq!(
            safe_rel(Path::new("ok/file")),
            Some(PathBuf::from("ok/file"))
        );
    }

    #[test]
    fn sha256_matches_known_input() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}

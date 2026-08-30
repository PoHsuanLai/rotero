use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub const REGISTRY_URL: &str =
    "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
const CACHE_TTL: Duration = Duration::from_secs(3 * 60 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Registry {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub agents: Vec<RegistryAgent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegistryAgent {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub distribution: Distribution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Distribution {
    #[serde(default)]
    pub binary: Option<std::collections::HashMap<String, BinaryDist>>,
    #[serde(default)]
    pub npx: Option<NpxDist>,
    #[serde(default)]
    pub uvx: Option<UvxDist>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinaryDist {
    pub archive: String,
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NpxDist {
    pub package: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UvxDist {
    pub package: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

/// Map ids written by older Rotero builds onto registry ids.
pub fn remap_provider_id(id: &str) -> String {
    match id {
        "claude" => "claude-acp".into(),
        "codex" => "codex-acp".into(),
        "copilot" => "github-copilot-cli".into(),
        other => other.to_string(),
    }
}

pub fn default_provider_id() -> String {
    "claude-acp".into()
}

fn cache_path() -> PathBuf {
    super::helpers::agent_working_dir().join("acp-registry.json")
}

fn cache_is_fresh(path: &std::path::Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < CACHE_TTL)
        .unwrap_or(false)
}

pub fn load_cached_registry() -> Option<Registry> {
    let path = cache_path();
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_cached_registry(registry: &Registry) -> Result<(), String> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create registry cache: {e}"))?;
    }
    let json = serde_json::to_string_pretty(registry).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("Failed to write registry cache: {e}"))?;
    Ok(())
}

/// Cached copy if it is still fresh; otherwise fetch. Falls back to a stale
/// cache when the network fails.
pub fn load_registry() -> Result<Registry, String> {
    let path = cache_path();
    if cache_is_fresh(&path)
        && let Some(cached) = load_cached_registry()
    {
        return Ok(cached);
    }
    match fetch_registry() {
        Ok(registry) => {
            let _ = save_cached_registry(&registry);
            Ok(registry)
        }
        Err(err) => load_cached_registry().ok_or(err),
    }
}

pub fn fetch_registry() -> Result<Registry, String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("rotero")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
    let resp = client
        .get(REGISTRY_URL)
        .send()
        .map_err(|e| format!("Failed to fetch ACP registry: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("ACP registry HTTP {}", resp.status()));
    }
    let registry: Registry = resp
        .json()
        .map_err(|e| format!("Failed to parse ACP registry: {e}"))?;
    if registry.agents.is_empty() {
        return Err("ACP registry listed no agents".into());
    }
    Ok(registry)
}

pub fn find_agent<'a>(registry: &'a Registry, id: &str) -> Option<&'a RegistryAgent> {
    let id = remap_provider_id(id);
    registry
        .agents
        .iter()
        .find(|a| a.id == id)
        .or_else(|| {
            registry
                .agents
                .iter()
                .find(|a| a.id == default_provider_id())
        })
        .or_else(|| registry.agents.first())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaps_legacy_provider_ids() {
        assert_eq!(remap_provider_id("claude"), "claude-acp");
        assert_eq!(remap_provider_id("codex"), "codex-acp");
        assert_eq!(remap_provider_id("copilot"), "github-copilot-cli");
        assert_eq!(remap_provider_id("gemini"), "gemini");
        assert_eq!(remap_provider_id("grok-build"), "grok-build");
    }

    #[test]
    fn parses_the_fixture_registry() {
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/acp_registry.json"
        ));
        let registry: Registry = serde_json::from_str(json).unwrap();
        assert_eq!(registry.agents.len(), 4);
        assert_eq!(registry.agents[0].id, "claude-acp");
        assert!(registry.agents[0].distribution.npx.is_some());
        assert_eq!(registry.agents[1].id, "grok-build");
        assert_eq!(
            registry.agents[1].distribution.npx.as_ref().unwrap().args,
            vec!["agent", "stdio"]
        );
        assert!(registry.agents[2].distribution.binary.is_some());
        assert!(registry.agents[3].distribution.uvx.is_some());
    }

    #[test]
    fn find_agent_falls_back_to_claude_then_first() {
        let json = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/acp_registry.json"
        ));
        let registry: Registry = serde_json::from_str(json).unwrap();
        assert_eq!(find_agent(&registry, "claude").unwrap().id, "claude-acp");
        assert_eq!(find_agent(&registry, "missing").unwrap().id, "claude-acp");
    }
}

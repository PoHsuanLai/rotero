use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SyncTransport {
    #[default]
    File,
    CloudKit,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PdfConfig {
    /// Default PDF zoom level (e.g. 0.75, 1.0, 1.5, 2.0, 3.0).
    #[serde(default = "default_zoom")]
    pub default_zoom: f32,
    #[serde(default = "default_page_batch_size")]
    pub page_batch_size: u32,
    #[serde(default = "default_selection_color")]
    pub selection_color: String,
    /// "png" or "jpeg".
    #[serde(default = "default_render_format")]
    pub render_format: String,
    /// 0-100.
    #[serde(default = "default_render_quality")]
    pub render_quality: u8,
    /// 0-100.
    #[serde(default = "default_thumbnail_quality")]
    pub thumbnail_quality: u8,
}

impl Default for PdfConfig {
    fn default() -> Self {
        Self {
            default_zoom: default_zoom(),
            page_batch_size: default_page_batch_size(),
            selection_color: default_selection_color(),
            render_format: default_render_format(),
            render_quality: default_render_quality(),
            thumbnail_quality: default_thumbnail_quality(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiConfig {
    #[serde(default)]
    pub dark_mode: bool,
    /// "compact", "default", or "comfortable".
    #[serde(default = "default_ui_scale")]
    pub ui_scale: String,
    #[serde(default = "default_annotation_color")]
    pub default_annotation_color: String,
    /// Persisted width (px) of the right-hand detail panel, set by dragging its
    /// resize handle. Clamped to the panel's CSS min/max (280–600).
    #[serde(default = "default_detail_width")]
    pub detail_width: f32,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            dark_mode: false,
            ui_scale: default_ui_scale(),
            default_annotation_color: default_annotation_color(),
            detail_width: default_detail_width(),
        }
    }
}

fn default_detail_width() -> f32 {
    360.0
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectorConfig {
    #[serde(default = "default_true")]
    pub connector_enabled: bool,
    #[serde(default = "default_connector_port")]
    pub connector_port: u16,
}

impl Default for ConnectorConfig {
    fn default() -> Self {
        Self {
            connector_enabled: default_true(),
            connector_port: default_connector_port(),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct FileSyncConfig {
    #[serde(default)]
    pub sync_enabled: bool,
    /// Path to the shared sync folder (e.g. iCloud Drive, Dropbox).
    #[serde(default)]
    pub sync_folder_path: Option<String>,
    #[serde(default)]
    pub sync_transport: SyncTransport,
    /// Custom library path (if set, overrides default app data dir).
    #[serde(default)]
    pub library_path: Option<String>,
    /// Path for auto-exported .bib file (Better BibTeX). None = disabled.
    #[serde(default)]
    pub auto_export_bib_path: Option<String>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentConfig {
    #[serde(default = "default_agent_provider")]
    pub agent_provider: String,
    #[serde(default)]
    pub agent_api_keys: std::collections::HashMap<String, String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            agent_provider: default_agent_provider(),
            agent_api_keys: std::collections::HashMap::new(),
        }
    }
}

/// Redacts the keys.
///
/// Written by hand rather than derived so that no future `{:?}` on a config —
/// a log line, an error message, a diagnostics dump — can print them. Nothing
/// does today; this is to keep it that way.
impl std::fmt::Debug for AgentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentConfig")
            .field("agent_provider", &self.agent_provider)
            .field(
                "agent_api_keys",
                &format!("<{} redacted>", self.agent_api_keys.len()),
            )
            .finish()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_true")]
    pub auto_check_updates: bool,
    #[serde(default)]
    pub last_check_timestamp: Option<i64>,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check_updates: default_true(),
            last_check_timestamp: None,
        }
    }
}

/// Persisted to config.json.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    #[serde(flatten)]
    pub pdf: PdfConfig,
    #[serde(flatten)]
    pub ui: UiConfig,
    #[serde(flatten)]
    pub connector: ConnectorConfig,
    #[serde(flatten)]
    pub sync: FileSyncConfig,
    #[serde(flatten)]
    pub agent: AgentConfig,
    #[serde(flatten)]
    pub update: UpdateConfig,

    #[serde(default = "default_true")]
    pub auto_fetch_metadata: bool,

    /// Tabs beyond this limit are suspended (pages cleared) to save memory.
    #[serde(default = "default_max_resident_tabs")]
    pub max_resident_tabs: u32,

    /// Cached device pixel ratio from the last run. Avoids async DPR race on startup.
    #[serde(default = "default_dpr")]
    pub cached_dpr: f32,

    /// User keybinding overrides: command id → key spec. Empty means "use the
    /// built-in defaults". See `crate::ui::keybindings`.
    ///
    /// Desktop-only: the keybinding module (native menus, muda) doesn't build on
    /// mobile. `#[serde(default)]` means the field simply round-trips absent
    /// there, so a config written on desktop still loads on mobile.
    #[cfg(feature = "desktop")]
    #[serde(default)]
    pub keybindings: crate::ui::keybindings::Overrides,
}

fn default_max_resident_tabs() -> u32 {
    3
}
fn default_dpr() -> f32 {
    1.0
}
fn default_agent_provider() -> String {
    crate::agent::registry::default_provider_id()
}

fn default_zoom() -> f32 {
    1.5
}
fn default_annotation_color() -> String {
    "#ffff00".to_string()
}
fn default_page_batch_size() -> u32 {
    5
}
fn default_selection_color() -> String {
    "#339af0".to_string()
}
fn default_render_format() -> String {
    "png".to_string()
}
fn default_render_quality() -> u8 {
    90
}
fn default_thumbnail_quality() -> u8 {
    60
}

fn default_ui_scale() -> String {
    "default".to_string()
}
fn default_true() -> bool {
    true
}
fn default_connector_port() -> u16 {
    21984
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            pdf: PdfConfig::default(),
            ui: UiConfig::default(),
            connector: ConnectorConfig::default(),
            sync: FileSyncConfig::default(),
            agent: AgentConfig::default(),
            update: UpdateConfig::default(),
            auto_fetch_metadata: default_true(),
            max_resident_tabs: default_max_resident_tabs(),
            cached_dpr: default_dpr(),
            #[cfg(feature = "desktop")]
            keybindings: crate::ui::keybindings::Overrides::default(),
        }
    }
}

impl SyncConfig {
    /// Load the config, falling back to defaults if it is missing or unreadable.
    ///
    /// A config that exists but does not parse is preserved under a
    /// `.corrupt-<timestamp>` name first. Returning defaults silently meant the
    /// next `save` overwrote it, destroying the user's library path and their
    /// plaintext agent API keys with no way back.
    pub fn load() -> Self {
        let path = config_path();
        if !path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Self>(&content) {
                Ok(mut config) => {
                    config.agent.agent_provider =
                        crate::agent::registry::remap_provider_id(&config.agent.agent_provider);
                    config
                }
                Err(e) => {
                    let stamp = chrono::Utc::now().format("%Y%m%d%H%M%S");
                    let backup = path.with_extension(format!("json.corrupt-{stamp}"));
                    let saved = std::fs::rename(&path, &backup).is_ok();
                    tracing::error!(
                        "Config at {} could not be parsed ({e}); {}",
                        path.display(),
                        if saved {
                            format!("preserved as {}", backup.display())
                        } else {
                            "and it could not be preserved".to_string()
                        }
                    );
                    #[cfg(feature = "desktop")]
                    crate::init::preflight::record(|p| {
                        p.config = Some(format!(
                            "settings could not be read and were reset{}",
                            if saved {
                                format!("; the previous file is at {}", backup.display())
                            } else {
                                String::new()
                            }
                        ));
                    });
                    Self::default()
                }
            },
            Err(e) => {
                tracing::error!("Config at {} could not be read: {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to save config: {e}"))?;
        // This file holds `agent_api_keys` in plaintext, and `fs::write` leaves
        // it world-readable — any other account or non-sandboxed process on the
        // machine could read them.
        restrict_permissions(&path);
        Ok(())
    }

    pub fn effective_library_path(&self) -> PathBuf {
        if let Some(ref custom) = self.sync.library_path {
            PathBuf::from(custom)
        } else {
            default_library_path()
        }
    }
}

fn config_path() -> PathBuf {
    app_support_dir().join("config.json")
}

fn default_library_path() -> PathBuf {
    app_support_dir()
}

/// The directory holding the database, `pdfs/`, `cache/`, and `config.json`.
///
/// `ROTERO_DATA_DIR` overrides the platform default, which points the whole
/// library — not just the database — somewhere else. The documentation
/// screenshot harness uses it to run against a seeded fixture library instead
/// of the real one.
fn app_support_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("ROTERO_DATA_DIR")
        && !dir.is_empty()
    {
        return PathBuf::from(dir);
    }
    platform_data_dir()
}

#[cfg(feature = "desktop")]
fn platform_data_dir() -> PathBuf {
    // Falls back rather than panicking: this runs before the window exists, so a
    // panic here is indistinguishable from the app crashing on launch.
    match directories::ProjectDirs::from("com", "rotero", "Rotero") {
        Some(dirs) => dirs.data_dir().to_path_buf(),
        None => {
            tracing::error!("Could not determine the platform data directory; using ~/.rotero");
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".rotero")
        }
    }
}

#[cfg(not(feature = "desktop"))]
fn platform_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Documents").join("Rotero")
}

pub fn check_external_modification(
    db_path: &Path,
    last_known_modified: Option<std::time::SystemTime>,
) -> bool {
    if let Some(last) = last_known_modified
        && let Ok(metadata) = std::fs::metadata(db_path)
        && let Ok(modified) = metadata.modified()
    {
        return modified > last;
    }
    false
}

pub fn file_modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Make a file readable only by its owner, where the platform has the concept.
///
/// `config.json` holds `agent_api_keys` in plaintext and `fs::write` creates it
/// world-readable.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `ROTERO_DATA_DIR` has to redirect the config and the library together —
    /// the screenshot harness relies on it to keep a capture run out of the
    /// real library. One test covers both, because the env var is process-wide
    /// and parallel tests would race on it.
    #[test]
    fn data_dir_env_override_redirects_config_and_library() {
        let default_config = {
            unsafe { std::env::remove_var("ROTERO_DATA_DIR") };
            config_path()
        };

        unsafe { std::env::set_var("ROTERO_DATA_DIR", "/tmp/rotero-fixture-test") };
        let overridden = PathBuf::from("/tmp/rotero-fixture-test");
        assert_eq!(app_support_dir(), overridden);
        assert_eq!(config_path(), overridden.join("config.json"));
        assert_eq!(default_library_path(), overridden);

        // An empty value is treated as unset, so an exported-but-blank variable
        // in a shell profile doesn't silently point the library at "".
        unsafe { std::env::set_var("ROTERO_DATA_DIR", "") };
        assert_eq!(config_path(), default_config);

        unsafe { std::env::remove_var("ROTERO_DATA_DIR") };
        assert_eq!(config_path(), default_config);
    }
}

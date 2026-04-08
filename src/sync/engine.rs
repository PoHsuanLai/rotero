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
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            dark_mode: false,
            ui_scale: default_ui_scale(),
            default_annotation_color: default_annotation_color(),
        }
    }
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
}

fn default_max_resident_tabs() -> u32 {
    3
}
fn default_agent_provider() -> String {
    "claude".to_string()
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
        }
    }
}

impl SyncConfig {
    pub fn load() -> Self {
        let path = config_path();
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(&path)
            && let Ok(config) = serde_json::from_str(&content)
        {
            return config;
        }
        Self::default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to save config: {e}"))?;
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

#[cfg(feature = "desktop")]
fn app_support_dir() -> PathBuf {
    let dirs = directories::ProjectDirs::from("com", "rotero", "Rotero")
        .expect("Could not determine data directory");
    dirs.data_dir().to_path_buf()
}

#[cfg(not(feature = "desktop"))]
fn app_support_dir() -> PathBuf {
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

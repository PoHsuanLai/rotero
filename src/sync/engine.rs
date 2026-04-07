use std::path::{Path, PathBuf};

/// Which sync transport to use.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SyncTransport {
    /// Sync via shared folder (iCloud Drive, Dropbox, etc.)
    #[default]
    File,
    /// Sync via Apple CloudKit (Apple devices only)
    CloudKit,
}

/// PDF viewer configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PdfConfig {
    /// Default PDF zoom level (e.g. 0.75, 1.0, 1.5, 2.0, 3.0).
    #[serde(default = "default_zoom")]
    pub default_zoom: f32,
    /// Number of PDF pages to load per batch.
    #[serde(default = "default_page_batch_size")]
    pub page_batch_size: u32,
    /// Text selection highlight color in the PDF viewer.
    #[serde(default = "default_selection_color")]
    pub selection_color: String,
    /// Render format for PDF pages (e.g. "png", "jpeg").
    #[serde(default = "default_render_format")]
    pub render_format: String,
    /// Render quality for PDF pages (0-100).
    #[serde(default = "default_render_quality")]
    pub render_quality: u8,
    /// Thumbnail quality (0-100).
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

/// UI appearance configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UiConfig {
    /// Enable dark mode.
    #[serde(default)]
    pub dark_mode: bool,
    /// UI density: "compact", "default", or "comfortable".
    #[serde(default = "default_ui_scale")]
    pub ui_scale: String,
    /// Default annotation highlight color.
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

/// Browser connector configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectorConfig {
    /// Whether the browser connector is enabled.
    #[serde(default = "default_true")]
    pub connector_enabled: bool,
    /// Browser connector port.
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

/// File sync / library path configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileSyncConfig {
    /// Whether CRR sync is enabled.
    #[serde(default)]
    pub sync_enabled: bool,
    /// Path to the shared sync folder (e.g. iCloud Drive, Dropbox).
    #[serde(default)]
    pub sync_folder_path: Option<String>,
    /// Which sync transport to use.
    #[serde(default)]
    pub sync_transport: SyncTransport,
    /// Custom library path (if set, overrides default app data dir).
    #[serde(default)]
    pub library_path: Option<String>,
    /// Path for auto-exported .bib file (Better BibTeX). None = disabled.
    #[serde(default)]
    pub auto_export_bib_path: Option<String>,
}

impl Default for FileSyncConfig {
    fn default() -> Self {
        Self {
            sync_enabled: false,
            sync_folder_path: None,
            sync_transport: SyncTransport::default(),
            library_path: None,
            auto_export_bib_path: None,
        }
    }
}

/// AI agent configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentConfig {
    /// Selected AI agent provider id.
    #[serde(default = "default_agent_provider")]
    pub agent_provider: String,
    /// API keys for agent providers (env var name -> value).
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

/// Application configuration, persisted to config.json.
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

    /// Auto-fetch metadata from CrossRef on PDF import.
    #[serde(default = "default_true")]
    pub auto_fetch_metadata: bool,

    /// Number of PDF tabs to keep rendered in memory for fast switching.
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
            auto_fetch_metadata: default_true(),
            max_resident_tabs: default_max_resident_tabs(),
        }
    }
}

impl SyncConfig {
    /// Load config from the default config file location.
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

    /// Save config to the default config file location.
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

    /// Get the effective library path (custom or default).
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

/// Returns the platform-appropriate app data directory.
#[cfg(feature = "desktop")]
fn app_support_dir() -> PathBuf {
    let dirs = directories::ProjectDirs::from("com", "rotero", "Rotero")
        .expect("Could not determine data directory");
    dirs.data_dir().to_path_buf()
}

/// On iOS/Android, use the app's sandboxed Documents directory.
#[cfg(not(feature = "desktop"))]
fn app_support_dir() -> PathBuf {
    // On iOS, the app sandbox HOME contains Documents/, Library/, etc.
    // We use the Documents dir so data persists and is accessible.
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Documents").join("Rotero")
}

/// Check if the database file was modified since our last known timestamp.
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

/// Get the modification time of a file.
pub fn file_modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

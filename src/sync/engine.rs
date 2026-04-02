use std::path::{Path, PathBuf};

/// Configuration for library sync.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncConfig {
    /// Custom library path (if set, overrides default app data dir).
    /// When pointed at a cloud-synced folder (Dropbox, iCloud, OneDrive),
    /// the library syncs automatically across devices.
    pub library_path: Option<String>,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            library_path: None,
        }
    }
}

impl SyncConfig {
    /// Load config from the default config file location.
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str(&content) {
                    return config;
                }
            }
        }
        Self::default()
    }

    /// Save config to the default config file location.
    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to save config: {e}"))?;
        Ok(())
    }

    /// Get the effective library path (custom or default).
    pub fn effective_library_path(&self) -> PathBuf {
        if let Some(ref custom) = self.library_path {
            PathBuf::from(custom)
        } else {
            default_library_path()
        }
    }
}

fn config_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("com", "rotero", "Rotero")
        .expect("Could not determine config directory");
    dirs.config_dir().join("config.json")
}

fn default_library_path() -> PathBuf {
    let dirs = directories::ProjectDirs::from("com", "rotero", "Rotero")
        .expect("Could not determine data directory");
    dirs.data_dir().to_path_buf()
}

/// Check if the database file was modified since our last known timestamp.
pub fn check_external_modification(db_path: &Path, last_known_modified: Option<std::time::SystemTime>) -> bool {
    if let Some(last) = last_known_modified {
        if let Ok(metadata) = std::fs::metadata(db_path) {
            if let Ok(modified) = metadata.modified() {
                return modified > last;
            }
        }
    }
    false
}

/// Get the modification time of a file.
pub fn file_modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

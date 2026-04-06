//! File-based changeset sync via shared folders (iCloud Drive, Dropbox, etc.)
//!
//! Sync folder layout:
//! ```text
//! {sync_folder}/
//!   changesets/          # .crr changeset files
//!   papers/              # PDF files (mirrored from library)
//!   sync_state.json      # per-peer tracking
//! ```

use std::path::{Path, PathBuf};

use rotero_db::crr::{self, ChangeRow};
use serde::{Deserialize, Serialize};

/// A serialized changeset file exchanged between devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    pub source_site_id: Vec<u8>,
    pub from_db_ver: i64,
    pub to_db_ver: i64,
    pub changes: Vec<ChangeRow>,
}

/// Tracks sync state: which changesets we've exported and which we've imported.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    /// Last db_version we exported.
    pub last_exported_ver: i64,
    /// Map of site_id (hex) → last imported db_version from that peer.
    pub peers: std::collections::HashMap<String, i64>,
}

/// The file-based sync engine.
pub struct FileSyncEngine {
    sync_folder: PathBuf,
    site_id: Vec<u8>,
}

impl FileSyncEngine {
    pub fn new(sync_folder: PathBuf, site_id: Vec<u8>) -> Self {
        Self {
            sync_folder,
            site_id,
        }
    }

    fn changesets_dir(&self) -> PathBuf {
        self.sync_folder.join("changesets")
    }

    fn state_path(&self) -> PathBuf {
        self.sync_folder.join("sync_state.json")
    }

    fn site_id_hex(&self) -> String {
        self.site_id.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Load sync state from disk.
    pub fn load_state(&self) -> SyncState {
        let path = self.state_path();
        if let Ok(content) = std::fs::read_to_string(&path) {
            match serde_json::from_str(&content) {
                Ok(state) => state,
                Err(e) => {
                    eprintln!("[sync] Failed to parse sync state at {}: {e}. Using defaults.", path.display());
                    SyncState::default()
                }
            }
        } else {
            SyncState::default()
        }
    }

    /// Save sync state to disk.
    pub fn save_state(&self, state: &SyncState) -> Result<(), String> {
        let path = self.state_path();
        let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("Failed to save sync state: {e}"))?;
        Ok(())
    }

    /// Export local changes since last export as a .crr file.
    /// Returns the number of changes exported, or 0 if nothing to export.
    pub async fn export_changes(
        &self,
        conn: &rotero_db::turso::Connection,
    ) -> Result<usize, String> {
        let mut state = self.load_state();
        let changes = crr::changes_since(conn, state.last_exported_ver)
            .await
            .map_err(|e| format!("Failed to read changes: {e}"))?;

        if changes.is_empty() {
            return Ok(0);
        }

        let current_ver = crr::current_db_version(conn)
            .await
            .map_err(|e| format!("Failed to read db_version: {e}"))?;

        let changeset = Changeset {
            source_site_id: self.site_id.clone(),
            from_db_ver: state.last_exported_ver,
            to_db_ver: current_ver,
            changes: changes.clone(),
        };

        // Ensure changesets dir exists
        let dir = self.changesets_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create changesets dir: {e}"))?;

        // Write changeset file
        let filename = format!(
            "{}_{:08}_{:08}.crr",
            self.site_id_hex(),
            state.last_exported_ver,
            current_ver,
        );
        let path = dir.join(&filename);
        let data =
            serde_json::to_vec(&changeset).map_err(|e| format!("Failed to serialize: {e}"))?;
        std::fs::write(&path, data).map_err(|e| format!("Failed to write changeset: {e}"))?;

        let count = changes.len();
        state.last_exported_ver = current_ver;
        self.save_state(&state)?;

        Ok(count)
    }

    /// Import any new .crr files from other peers.
    /// Returns the total number of changes applied.
    pub async fn import_changes(
        &self,
        conn: &rotero_db::turso::Connection,
    ) -> Result<usize, String> {
        let dir = self.changesets_dir();
        if !dir.exists() {
            return Ok(0);
        }

        let my_hex = self.site_id_hex();
        let mut state = self.load_state();
        let mut total_applied = 0;

        // Read all .crr files
        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("Failed to read changesets dir: {e}"))?;

        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "crr"))
            .collect();
        files.sort(); // deterministic order

        for path in files {
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            // Skip our own changesets
            if filename.starts_with(&my_hex) {
                continue;
            }

            // Parse site_id from filename: {site_hex}_{from}_{to}
            let parts: Vec<&str> = filename.splitn(3, '_').collect();
            if parts.len() < 3 {
                continue;
            }
            let peer_hex = parts[0];
            let to_ver: i64 = parts[2].parse().unwrap_or(0);

            // Skip if we've already imported up to this version from this peer
            let last_imported = state.peers.get(peer_hex).copied().unwrap_or(0);
            if to_ver <= last_imported {
                continue;
            }

            // Read and deserialize
            let data = tokio::fs::read(&path)
                .await
                .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
            let changeset: Changeset = serde_json::from_slice(&data)
                .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;

            // Apply
            let result = crr::apply_changes(conn, &changeset.changes)
                .await
                .map_err(|e| format!("Failed to apply changes: {e}"))?;

            total_applied += result.applied;

            // Update peer tracking
            state
                .peers
                .insert(peer_hex.to_string(), changeset.to_db_ver);
        }

        self.save_state(&state)?;
        Ok(total_applied)
    }

    /// Copy a PDF file to the sync folder's papers/ directory.
    pub fn export_pdf(&self, library_papers_dir: &Path, rel_path: &str) -> Result<(), String> {
        let src = library_papers_dir.join(rel_path);
        if !src.exists() {
            return Ok(()); // nothing to sync
        }

        let dest_dir = self.sync_folder.join("papers");
        let dest = dest_dir.join(rel_path);
        if dest.exists() {
            return Ok(()); // already synced
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create sync papers dir: {e}"))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| format!("Failed to copy PDF to sync: {e}"))?;
        Ok(())
    }

    /// Import a PDF from the sync folder into the local library.
    pub fn import_pdf(&self, library_papers_dir: &Path, rel_path: &str) -> Result<(), String> {
        let src = self.sync_folder.join("papers").join(rel_path);
        if !src.exists() {
            return Ok(()); // not available yet
        }

        let dest = library_papers_dir.join(rel_path);
        if dest.exists() {
            return Ok(()); // already have it
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create local papers dir: {e}"))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| format!("Failed to import PDF from sync: {e}"))?;
        Ok(())
    }
}

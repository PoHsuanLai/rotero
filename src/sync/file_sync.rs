//! File-based changeset sync via shared folders (iCloud Drive, Dropbox, etc.)
//!
//! Sync folder layout: `{sync_folder}/changesets/` (.crr files),
//! `papers/` (mirrored PDFs), `sync_state.json` (per-peer tracking).

use std::path::{Path, PathBuf};

use rotero_db::Database;
use rotero_db::crr::ChangeRow;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Changeset {
    pub source_site_id: Vec<u8>,
    pub from_db_ver: i64,
    pub to_db_ver: i64,
    pub changes: Vec<ChangeRow>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    pub last_exported_ver: i64,
    /// Map of site_id (hex) -> last imported db_version from that peer.
    pub peers: std::collections::HashMap<String, i64>,
}

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

    /// Where this device records its own sync progress.
    ///
    /// Qualified by site id because every field in [`SyncState`] is private to
    /// one device: `last_exported_ver` counts local writes, and `peers` records
    /// what *this* device has imported. A shared file let the fastest device
    /// park the cursor at its own version, after which every slower device read
    /// that number, found nothing newer than it, and exported nothing — while
    /// reporting success. The cursor only ever moves up, so an affected device
    /// never recovered on its own.
    fn state_path(&self) -> PathBuf {
        self.sync_folder
            .join(format!("sync_state_{}.json", self.site_id_hex()))
    }

    /// The pre-fix shared path, still read once so an existing install keeps its
    /// progress instead of re-exporting its whole library.
    fn legacy_state_path(&self) -> PathBuf {
        self.sync_folder.join("sync_state.json")
    }

    fn site_id_hex(&self) -> String {
        self.site_id.iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn load_state(&self) -> SyncState {
        let path = self.state_path();
        if path.exists() {
            return Self::read_state(&path);
        }

        // First run since the per-device split. The shared file's `peers` map is
        // still useful — it records changesets someone already imported, and
        // re-importing them is wasteful but harmless. `last_exported_ver` is not:
        // it may belong to a different device entirely, and adopting it is what
        // starved this one. Dropping it to 0 re-exports this device's history,
        // which is exactly the repair an affected install needs.
        let legacy = self.legacy_state_path();
        if legacy.exists() {
            let shared = Self::read_state(&legacy);
            tracing::info!(
                "Sync: adopting per-device state; re-exporting from 0 (shared cursor was {})",
                shared.last_exported_ver
            );
            return SyncState {
                last_exported_ver: 0,
                peers: shared.peers,
            };
        }

        SyncState::default()
    }

    fn read_state(path: &Path) -> SyncState {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str(&content) {
                Ok(state) => state,
                Err(e) => {
                    tracing::warn!(
                        "Failed to parse sync state at {}: {e}. Using defaults.",
                        path.display()
                    );
                    SyncState::default()
                }
            },
            Err(_) => SyncState::default(),
        }
    }

    pub fn save_state(&self, state: &SyncState) -> Result<(), String> {
        let path = self.state_path();
        // The sync folder may not exist yet — a cloud provider often has not
        // materialized it on a machine's first run — and without this the write
        // failed every 30 seconds with nothing to show for it.
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create sync state dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
        // Write-then-rename: these files live in a folder a cloud daemon is
        // actively syncing, and a partial `write` there is a file that parses as
        // garbage on the next read.
        write_atomic(&path, json.as_bytes()).map_err(|e| format!("Failed to save sync state: {e}"))
    }

    /// Returns the number of changes exported, or 0 if nothing to export.
    pub async fn export_changes(&self, db: &Database) -> Result<usize, String> {
        let mut state = self.load_state();
        let changes = db
            .crr()
            .changes_since(state.last_exported_ver)
            .await
            .map_err(|e| format!("Failed to read changes: {e}"))?;

        if changes.is_empty() {
            return Ok(0);
        }

        let current_ver = db
            .crr()
            .current_db_version()
            .await
            .map_err(|e| format!("Failed to read db_version: {e}"))?;

        let changeset = Changeset {
            source_site_id: self.site_id.clone(),
            from_db_ver: state.last_exported_ver,
            to_db_ver: current_ver,
            changes: changes.clone(),
        };

        let dir = self.changesets_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create changesets dir: {e}"))?;

        let filename = format!(
            "{}_{:08}_{:08}.crr",
            self.site_id_hex(),
            state.last_exported_ver,
            current_ver,
        );
        let path = dir.join(&filename);
        let data =
            serde_json::to_vec(&changeset).map_err(|e| format!("Failed to serialize: {e}"))?;
        // A peer scans for `*.crr` and parses whatever it finds, so a half-written
        // file is a parse error that aborts that peer's whole import pass.
        write_atomic(&path, &data).map_err(|e| format!("Failed to write changeset: {e}"))?;

        let count = changes.len();
        state.last_exported_ver = current_ver;
        self.save_state(&state)?;

        Ok(count)
    }

    /// Returns the total number of changes applied.
    pub async fn import_changes(&self, db: &Database) -> Result<usize, String> {
        let dir = self.changesets_dir();
        if !dir.exists() {
            return Ok(0);
        }

        let my_hex = self.site_id_hex();
        let mut state = self.load_state();
        let mut total_applied = 0;

        let entries =
            std::fs::read_dir(&dir).map_err(|e| format!("Failed to read changesets dir: {e}"))?;

        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "crr"))
            .collect();
        files.sort();

        for path in files {
            let filename = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

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

            let last_imported = state.peers.get(peer_hex).copied().unwrap_or(0);
            if to_ver <= last_imported {
                continue;
            }

            let data = tokio::fs::read(&path)
                .await
                .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
            let changeset: Changeset = serde_json::from_slice(&data)
                .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;

            let result = db
                .crr()
                .apply_changes(&changeset.changes)
                .await
                .map_err(|e| format!("Failed to apply changes: {e}"))?;

            total_applied += result.applied;

            state
                .peers
                .insert(peer_hex.to_string(), changeset.to_db_ver);
        }

        self.save_state(&state)?;
        Ok(total_applied)
    }

    pub fn export_pdf(&self, library_papers_dir: &Path, rel_path: &str) -> Result<(), String> {
        let Some(safe) = safe_relative_path(rel_path) else {
            return Err(format!("Refusing to sync PDF at unsafe path {rel_path:?}"));
        };

        let src = library_papers_dir.join(&safe);
        if !src.exists() {
            return Ok(());
        }

        let dest = self.sync_folder.join("papers").join(&safe);
        if dest.exists() {
            return Ok(());
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create sync papers dir: {e}"))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| format!("Failed to copy PDF to sync: {e}"))?;
        Ok(())
    }

    pub fn import_pdf(&self, library_papers_dir: &Path, rel_path: &str) -> Result<(), String> {
        let Some(safe) = safe_relative_path(rel_path) else {
            return Err(format!(
                "Refusing to import PDF at unsafe path {rel_path:?}"
            ));
        };

        let src = self.sync_folder.join("papers").join(&safe);
        if !src.exists() {
            return Ok(());
        }

        let dest = library_papers_dir.join(&safe);
        if dest.exists() {
            return Ok(());
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create local papers dir: {e}"))?;
        }
        std::fs::copy(&src, &dest).map_err(|e| format!("Failed to import PDF from sync: {e}"))?;
        Ok(())
    }
}

/// A peer-supplied path, accepted only if it stays inside the directory it is
/// joined onto.
///
/// `pdf_path` arrives over sync, so it is whatever another device wrote. It is
/// then joined onto the papers directory — and `Path::join` with an absolute
/// path *replaces* the base rather than extending it, so `/etc/foo` or
/// `../../../.zshrc` would read and write outside the library entirely. Only
/// ordinary path components are allowed: no root, no prefix, no `..`, and no
/// bare `.`.
fn safe_relative_path(rel_path: &str) -> Option<PathBuf> {
    if rel_path.is_empty() {
        return None;
    }

    let mut out = PathBuf::new();
    for component in Path::new(rel_path).components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            // Anything else can leave the directory, so reject the whole path
            // rather than silently dropping the offending component.
            _ => return None,
        }
    }

    (!out.as_os_str().is_empty()).then_some(out)
}

/// Write a file by creating a sibling temporary and renaming it into place.
///
/// Everything here lives in a folder a cloud daemon watches and uploads. A plain
/// `write` is observable half-finished, and a truncated `.crr` is a parse error
/// that aborts a peer's entire import pass. The rename is atomic because the
/// temporary is in the same directory, hence the same filesystem.
fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let tmp_name = format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("state")
    );
    let tmp = dir.join(tmp_name);

    std::fs::write(&tmp, data)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(folder: &Path, site: u8) -> FileSyncEngine {
        FileSyncEngine::new(folder.to_path_buf(), vec![site; 16])
    }

    /// Two devices must not share one export cursor.
    ///
    /// The shared file let the busier device park the cursor at its own
    /// `db_ver`; the other then read that number, found nothing newer, and
    /// exported nothing while reporting success — permanently, since the cursor
    /// only moves up.
    #[test]
    fn each_device_keeps_its_own_export_cursor() {
        let dir = tempfile::tempdir().unwrap();

        let a = engine(dir.path(), 1);
        a.save_state(&SyncState {
            last_exported_ver: 40,
            peers: Default::default(),
        })
        .unwrap();

        let b = engine(dir.path(), 2);
        assert_eq!(
            b.load_state().last_exported_ver,
            0,
            "a second device must not inherit the first device's cursor"
        );

        // And writing B's state must not disturb A's.
        b.save_state(&SyncState {
            last_exported_ver: 7,
            peers: Default::default(),
        })
        .unwrap();
        assert_eq!(a.load_state().last_exported_ver, 40);
    }

    /// An existing install keeps what its peers already delivered, but re-exports
    /// its own history — the shared cursor may have belonged to another device,
    /// and trusting it is what starved this one.
    #[test]
    fn a_shared_state_file_is_adopted_without_its_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let mut peers = std::collections::HashMap::new();
        peers.insert("aa".to_string(), 12);
        let legacy = serde_json::to_string(&SyncState {
            last_exported_ver: 99,
            peers,
        })
        .unwrap();
        std::fs::write(dir.path().join("sync_state.json"), legacy).unwrap();

        let state = engine(dir.path(), 1).load_state();
        assert_eq!(
            state.last_exported_ver, 0,
            "the shared cursor must not be adopted"
        );
        assert_eq!(
            state.peers.get("aa"),
            Some(&12),
            "already-imported peer progress is still valid and worth keeping"
        );
    }

    /// `pdf_path` arrives from a peer, and `Path::join` with an absolute path
    /// replaces the base instead of extending it.
    #[test]
    fn peer_supplied_paths_cannot_escape_the_papers_directory() {
        for escape in [
            "../../../.zshrc",
            "/etc/passwd",
            "a/../../b.pdf",
            "..",
            "",
            "./x.pdf",
        ] {
            assert!(
                safe_relative_path(escape).is_none(),
                "{escape:?} must be rejected"
            );
        }

        assert_eq!(
            safe_relative_path("2024/paper.pdf"),
            Some(PathBuf::from("2024/paper.pdf")),
            "an ordinary relative path must still work"
        );
    }

    /// A reader must never observe a partially written file: peers scan for
    /// `*.crr` and abort their whole import pass on one parse error.
    #[test]
    fn writes_are_atomic_and_leave_no_temporary_behind() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");

        write_atomic(&path, b"first").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"first");

        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");

        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "state.json")
            .collect();
        assert!(leftovers.is_empty(), "stray temp files: {leftovers:?}");
    }
}

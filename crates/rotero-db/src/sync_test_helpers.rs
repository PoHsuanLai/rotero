//! Test helpers for simulating multi-device sync.
//!
//! Mirrors the real transport's shape — one snapshot file per device in a shared
//! directory — without its cloud-provider handling, so tests exercise the same
//! merge the app runs.

use std::path::PathBuf;

use crate::Database;

/// Simulates a sync endpoint by exchanging per-device snapshots in a directory.
///
/// Keeps `export_changes`/`import_changes` names from the changeset era so the
/// test harnesses built around them did not have to be rewritten alongside the
/// engine.
pub struct TestSyncEngine {
    dir: PathBuf,
    site_id: Vec<u8>,
}

impl TestSyncEngine {
    /// Create a new engine for the given shared directory and device site ID.
    pub fn new(dir: PathBuf, site_id: Vec<u8>) -> Self {
        Self { dir, site_id }
    }

    fn site_hex(&self) -> String {
        self.site_id.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn devices_dir(&self) -> PathBuf {
        self.dir.join("devices")
    }

    /// Write this device's snapshot. Returns the number of rows written.
    pub async fn export_changes(&self, db: &Database) -> usize {
        let dir = self.devices_dir();
        std::fs::create_dir_all(&dir).unwrap();

        let bytes = db.write_snapshot().await.unwrap();
        let (header, _) = crate::snapshot::parse_snapshot(&bytes).unwrap();

        std::fs::write(dir.join(format!("{}.snapshot", self.site_hex())), &bytes).unwrap();
        header.rows
    }

    /// Merge every other device's snapshot. Returns the number of rows applied.
    pub async fn import_changes(&self, db: &Database) -> usize {
        let dir = self.devices_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };

        let mine = format!("{}.snapshot", self.site_hex());
        let mut total = 0;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == mine.as_str()) {
                continue;
            }
            if path.extension().is_none_or(|e| e != "snapshot") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            total += db.merge_snapshot(&bytes).await.unwrap().applied;
        }
        total
    }
}

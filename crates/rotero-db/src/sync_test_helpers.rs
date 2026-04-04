//! Test helpers for simulating multi-device sync without the full FileSyncEngine.

use std::path::PathBuf;

use crate::crr::{self, ChangeRow};

/// Minimal sync engine for integration tests.
pub struct TestSyncEngine {
    dir: PathBuf,
    site_id: Vec<u8>,
}

impl TestSyncEngine {
    pub fn new(dir: PathBuf, site_id: Vec<u8>) -> Self {
        Self { dir, site_id }
    }

    fn site_hex(&self) -> String {
        self.site_id.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Export changes to a JSON file in the test dir. Returns count exported.
    pub async fn export_changes(&self, conn: &turso::Connection) -> usize {
        let state_path = self.dir.join(format!("{}_state.json", self.site_hex()));
        let last_ver: i64 = std::fs::read_to_string(&state_path)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let changes = crr::changes_since(conn, last_ver).await.unwrap();
        if changes.is_empty() {
            return 0;
        }

        let current_ver = crr::current_db_version(conn).await.unwrap();
        let filename = format!("{}_{:08}_{:08}.json", self.site_hex(), last_ver, current_ver);
        let path = self.dir.join(&filename);
        let data = serde_json::to_vec(&changes).unwrap();
        std::fs::write(&path, data).unwrap();
        std::fs::write(&state_path, current_ver.to_string()).unwrap();

        changes.len()
    }

    /// Import changes from other devices' JSON files. Returns count applied.
    pub async fn import_changes(&self, conn: &turso::Connection) -> usize {
        let my_hex = self.site_hex();
        let mut total = 0;

        let entries: Vec<_> = std::fs::read_dir(&self.dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "json")
                    && !p.file_name().unwrap().to_string_lossy().contains("_state")
                    && !p.file_name().unwrap().to_string_lossy().starts_with(&my_hex)
            })
            .collect();

        for path in entries {
            let data = std::fs::read(&path).unwrap();
            let changes: Vec<ChangeRow> = serde_json::from_slice(&data).unwrap();
            let result = crr::apply_changes(conn, &changes).await.unwrap();
            total += result.applied;
        }

        total
    }
}

pub mod engine;
pub mod file_sync;

#[cfg(feature = "cloudkit")]
pub mod cloudkit_sync;

use rotero_db::turso::Connection;

/// Common interface for sync backends (file-based, CloudKit, etc.).
pub trait SyncBackend {
    fn export_changes(
        &mut self,
        conn: &Connection,
    ) -> impl std::future::Future<Output = Result<usize, String>> + Send;

    fn import_changes(
        &mut self,
        conn: &Connection,
    ) -> impl std::future::Future<Output = Result<usize, String>> + Send;
}

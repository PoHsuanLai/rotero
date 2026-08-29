/// The process-wide library database, opened once before the UI starts.
///
/// Holds a fully initialized [`rotero_db::Database`] rather than a bare
/// connection: the background connector and MCP threads clone it, and handing
/// them a connection meant each had to reconstruct the CRR store itself. One of
/// those reconstructions skipped `crr.init()`, which committed rows and then
/// failed change tracking — silently losing tags and notes.
#[cfg(feature = "desktop")]
pub static SHARED_DB: std::sync::OnceLock<rotero_db::Database> = std::sync::OnceLock::new();

/// Why the database could not be opened, when it could not be.
///
/// The window still launches on failure so the app can render the error; without
/// this the process exited and a GUI user saw only a silent bounce, with the
/// reason in a log file they had no reason to open.
#[cfg(feature = "desktop")]
pub static DB_INIT_ERROR: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Whether [`DB_INIT_ERROR`] is version skew rather than a damaged library.
///
/// Captured at startup because it is only readable immediately after the failed
/// open. The error screen uses it to offer an in-app update — the message tells
/// the user to update, and until this existed that was the one state in which
/// the updater never ran, because it mounts only on the success path.
#[cfg(feature = "desktop")]
pub static DB_INIT_NEEDS_UPDATE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Open the library database, creating and migrating it if needed.
///
/// Delegates to [`rotero_db::Database::open`] so the desktop app and the tests
/// exercise the same initialization. Do not reimplement the open sequence here:
/// a second copy is what shipped a database without CRR metadata.
#[cfg(feature = "desktop")]
pub(crate) async fn init_database(
    config: &crate::sync::engine::SyncConfig,
) -> Result<rotero_db::Database, String> {
    rotero_db::Database::open(config.effective_library_path()).await
}

#[cfg(test)]
mod tests {
    /// `init_database` must delegate rather than re-derive the open sequence.
    ///
    /// Crude, but it is the cheap backstop for the specific regression that
    /// shipped: an `init_database` that called `initialize_db` directly and so
    /// never ran `crr.init()`.
    #[test]
    fn init_database_delegates_to_database_open() {
        // Only the code above this test module: comments name these functions
        // deliberately, and so do the assertion messages below.
        let src = include_str!("database.rs");
        let code: String = src
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            code.contains("Database::open"),
            "init_database must delegate to Database::open"
        );
        assert!(
            !code.contains("initialize_db("),
            "init_database must not call initialize_db directly — that bypasses crr.init()"
        );
    }
}

//! Startup health, collected from checks that already run.
//!
//! Every field here records the outcome of something startup was doing anyway —
//! opening the database, binding the connector and MCP ports. Those results used
//! to be logged and dropped, so a user whose connector never bound, or whose
//! library was missing its sync metadata, saw a working-looking app and a log
//! file they had no reason to read.
//!
//! Populating this costs nothing at startup; it only stops throwing the answers
//! away.

use std::sync::{OnceLock, RwLock};

/// What went wrong at startup, if anything. `None` means the check passed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Preflight {
    /// The library database could not be opened, or is structurally unsound.
    pub db: Option<String>,
    /// PDF rendering is unavailable; the engine could not be bound.
    pub pdf_engine: Option<String>,
    /// The browser connector could not bind its port.
    pub connector_port: Option<String>,
    /// The MCP server could not bind its port.
    pub mcp_port: Option<String>,
    /// The configured sync folder is unusable.
    pub sync_folder: Option<String>,
    /// Settings could not be read and were reset to defaults.
    pub config: Option<String>,
}

impl Preflight {
    /// Whether every check passed.
    pub fn is_healthy(&self) -> bool {
        self.issues().is_empty()
    }

    /// Each failure, paired with the name of the subsystem it belongs to.
    pub fn issues(&self) -> Vec<(&'static str, &str)> {
        [
            ("Library", self.db.as_deref()),
            ("PDF engine", self.pdf_engine.as_deref()),
            ("Browser connector", self.connector_port.as_deref()),
            ("MCP server", self.mcp_port.as_deref()),
            ("Sync", self.sync_folder.as_deref()),
            ("Settings", self.config.as_deref()),
        ]
        .into_iter()
        .filter_map(|(name, msg)| msg.map(|m| (name, m)))
        .collect()
    }
}

fn cell() -> &'static RwLock<Preflight> {
    static CELL: OnceLock<RwLock<Preflight>> = OnceLock::new();
    CELL.get_or_init(|| RwLock::new(Preflight::default()))
}

/// Record a startup result. Later checks may run on background threads, so this
/// is callable from anywhere.
pub fn record(f: impl FnOnce(&mut Preflight)) {
    if let Ok(mut p) = cell().write() {
        f(&mut p);
    }
}

/// A snapshot of startup health.
pub fn snapshot() -> Preflight {
    cell().read().map(|p| p.clone()).unwrap_or_default()
}

/// Check the opened database's structural invariants and record any problems.
///
/// Runs the same [`verify_database_health`](rotero_db::health::verify_database_health)
/// the tests and the bundle smoke check use, so a startup path that skips part
/// of initialization is caught in the field as well as in CI.
#[cfg(feature = "desktop")]
pub async fn check_database(db: &rotero_db::Database) {
    let issues = rotero_db::health::verify_database_health(db).await;
    if issues.is_empty() {
        return;
    }
    let detail = issues
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    tracing::error!("Database health check failed: {detail}");
    record(|p| p.db = Some(detail));
}

//! Structural invariants an initialized Rotero database must satisfy.
//!
//! This is the single definition of "correctly initialized". [`Database::open`]
//! is not the only thing that has ever constructed a `Database`, and when a
//! second construction path skipped [`CrrStore::init`], every write committed its
//! row and *then* failed on change tracking — silently losing tags and notes and
//! leaving the library unable to sync. The tests all passed, because they built
//! their fixtures through the path that was correct.
//!
//! [`verify_database_health`] exists so that cannot recur silently. It is called
//! by the startup preflight, by the bundle smoke test, and by the unit test that
//! pins the invariant, so a construction path that skips a step cannot satisfy
//! one caller and fail another.
//!
//! The table list is derived from [`crr::rotero_schema`], never hardcoded: a new
//! `#[derive(Crdt)]` table extends the invariant automatically.

use crate::{Database, crr};

/// The shadow clock table recrr maintains for a tracked table.
///
/// Mirrors recrr's own `clock_table` helper, which is crate-private there.
fn clock_table(table: &str) -> String {
    format!("{table}__crr_clock")
}

/// A structural problem with an initialized database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthIssue {
    /// CRR metadata tables are absent entirely: `crr.init()` was never called.
    /// Every mutating call will commit its row and then fail change tracking.
    CrrUninitialized,
    /// A tracked table has no `{table}__crr_clock` shadow table. Writes to it
    /// commit and then error, and its rows never sync.
    MissingCrrClock {
        /// The tracked table whose clock table is missing.
        table: String,
    },
    /// A table named by the compiled schema does not exist in the database.
    MissingTable {
        /// The absent table.
        table: String,
    },
    /// The persisted CRR fingerprint disagrees with the compiled schema, so
    /// peers are tracking a different set of tables or columns than this build.
    SchemaFingerprintDrift {
        /// The fingerprint recorded in the database.
        stored: u64,
        /// The fingerprint of the schema this build was compiled with.
        expected: u64,
    },
    /// `schema_version` is absent, zero, or ahead of what this build supports.
    SchemaVersion {
        /// The version recorded in the database.
        found: i64,
        /// The version this build expects.
        expected: i64,
    },
}

impl std::fmt::Display for HealthIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CrrUninitialized => write!(
                f,
                "sync metadata was never initialized; edits will not be saved or synced"
            ),
            Self::MissingCrrClock { table } => {
                write!(f, "sync tracking is missing for the `{table}` table")
            }
            Self::MissingTable { table } => write!(f, "the `{table}` table is missing"),
            Self::SchemaFingerprintDrift { stored, expected } => write!(
                f,
                "sync schema mismatch (library {stored:x}, this version {expected:x})"
            ),
            Self::SchemaVersion { found, expected } => write!(
                f,
                "library schema version {found} does not match this version's {expected}"
            ),
        }
    }
}

/// Whether `name` exists as a table in the database.
async fn table_exists(db: &Database, name: &str) -> Result<bool, turso::Error> {
    let mut rows = db
        .conn()
        .query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [turso::Value::Text(name.to_string())],
        )
        .await?;
    Ok(rows.next().await?.is_some())
}

/// Check every structural invariant a usable Rotero database must satisfy.
///
/// Returns an empty vector for a healthy database. A driver error is reported as
/// a health issue rather than propagated: this runs at startup and during
/// diagnostics, where "the check itself failed" is a finding, not a reason to
/// abort. Ordered most-fundamental first, so the first entry is the one worth
/// showing when only one line fits.
pub async fn verify_database_health(db: &Database) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    let schema = crr::rotero_schema();

    // Without the metadata tables nothing else is meaningful: every tracked
    // table's clock is missing too, and reporting nine follow-on issues would
    // bury the cause.
    match table_exists(db, "crr_site_id").await {
        Ok(true) => {}
        Ok(false) => {
            issues.push(HealthIssue::CrrUninitialized);
            return issues;
        }
        Err(_) => {
            issues.push(HealthIssue::CrrUninitialized);
            return issues;
        }
    }

    for table in &schema.tables {
        if !table_exists(db, &table.name).await.unwrap_or(false) {
            issues.push(HealthIssue::MissingTable {
                table: table.name.clone(),
            });
        }
        if !table_exists(db, &clock_table(&table.name))
            .await
            .unwrap_or(false)
        {
            issues.push(HealthIssue::MissingCrrClock {
                table: table.name.clone(),
            });
        }
    }

    // A stored fingerprint only exists once `init` has run; its absence is
    // already covered by the `crr_site_id` check above.
    let expected = schema.fingerprint();
    if let Some(stored) = db.crr().schema_fingerprint().await
        && stored != expected
    {
        issues.push(HealthIssue::SchemaFingerprintDrift { stored, expected });
    }

    let expected = crate::schema::migrations::SCHEMA_VERSION;
    match crate::schema::migrations::get_schema_version(db.conn()).await {
        Ok(found) if found != expected => {
            issues.push(HealthIssue::SchemaVersion { found, expected });
        }
        Ok(_) => {}
        // Unreadable is its own problem: reporting it as version 0 is what let a
        // populated library be treated as fresh.
        Err(_) => issues.push(HealthIssue::SchemaVersion {
            found: -1,
            expected,
        }),
    }

    issues
}

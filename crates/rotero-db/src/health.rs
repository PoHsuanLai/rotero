//! Structural invariants an initialized Rotero database must satisfy.
//!
//! This is the single definition of "correctly initialized". [`Database::open`]
//! is not the only thing that has ever constructed a `Database`, and when a
//! second construction path skipped the sync store's setup, every write
//! committed its row and *then* failed on change tracking — silently losing tags
//! and notes and leaving the library unable to sync. The tests all passed,
//! because they built their fixtures through the path that was correct.
//!
//! [`verify_database_health`] exists so that cannot recur silently. It is called
//! by the startup preflight, by the bundle smoke test, and by the unit test that
//! pins the invariant, so a construction path that skips a step cannot satisfy
//! one caller and fail another.
//!
//! The shape of the failure changed with the sync engine but not its character.
//! Where a row used to be lost because its clock table was missing, a row is now
//! lost because its clock *columns* are unset: `updated_at = 0` loses every
//! comparison a merge makes, forever. It is present locally, looks correct in
//! the UI, and can never reach another device — the same silent loss, so it is
//! checked the same way.
//!
//! The table list is derived from [`sync_schema::SYNCED_TABLES`], never
//! hardcoded: a new synced table extends the invariant automatically.

use crate::Database;
use crate::sync_schema::SYNCED_TABLES;

/// A structural problem with an initialized database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthIssue {
    /// The library has no device identity, so nothing it writes can be
    /// attributed and every merge tie breaks against it.
    DeviceIdMissing,
    /// A synced table is missing the columns a merge compares. Writes to it
    /// cannot be stamped, and its rows never sync.
    MissingSyncColumns {
        /// The synced table.
        table: String,
        /// The columns it lacks.
        columns: Vec<String>,
    },
    /// A table named by the compiled manifest does not exist in the database.
    MissingTable {
        /// The absent table.
        table: String,
    },
    /// The database has a column the manifest does not name, or vice versa, so
    /// peers are syncing a different set of columns than this build.
    SyncSchemaDrift {
        /// The table the mismatch is in.
        table: String,
        /// The column that is present on one side only.
        column: String,
    },
    /// `schema_version` is absent, zero, or ahead of what this build supports.
    SchemaVersion {
        /// The version recorded in the database.
        found: i64,
        /// The version this build expects.
        expected: i64,
    },
    /// Rows carry no sync clock, so they lose every merge and never propagate.
    ///
    /// The direct heir to the original bug. The columns exist and the table
    /// looks healthy; the rows inside it simply cannot win a comparison, which
    /// is indistinguishable from working until a second device disagrees.
    UnstampedRows {
        /// The table holding them.
        table: String,
        /// How many rows are unstamped.
        rows: i64,
    },
    /// A tombstone with no clock: a deletion that can never reach a peer.
    ///
    /// Worse than an unstamped live row, because the row is already invisible
    /// locally — the delete looks done on this device and silently never
    /// happens on any other.
    TombstoneWithoutClock {
        /// The table holding them.
        table: String,
        /// How many tombstones cannot propagate.
        rows: i64,
    },
}

impl std::fmt::Display for HealthIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceIdMissing => write!(
                f,
                "this library has no device identity; its changes cannot be synced"
            ),
            Self::MissingSyncColumns { table, columns } => write!(
                f,
                "the `{table}` table is missing sync columns ({})",
                columns.join(", ")
            ),
            Self::MissingTable { table } => write!(f, "the `{table}` table is missing"),
            Self::SyncSchemaDrift { table, column } => write!(
                f,
                "sync schema mismatch: `{table}`.`{column}` is not synced by this version"
            ),
            Self::SchemaVersion { found, expected } => write!(
                f,
                "library schema version {found} does not match this version's {expected}"
            ),
            Self::UnstampedRows { table, rows } => write!(
                f,
                "{rows} row(s) in `{table}` have no sync clock and will not sync"
            ),
            Self::TombstoneWithoutClock { table, rows } => write!(
                f,
                "{rows} deletion(s) in `{table}` cannot reach other devices"
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

/// The columns a table actually has.
async fn actual_columns(db: &Database, table: &str) -> Result<Vec<String>, turso::Error> {
    let mut rows = db
        .conn()
        .query(&format!("PRAGMA table_info({table})"), ())
        .await?;
    let mut names = Vec::new();
    while let Some(row) = rows.next().await? {
        if let Some(name) = row.get_value(1).ok().and_then(|v| v.as_text().cloned()) {
            names.push(name);
        }
    }
    Ok(names)
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

    // Without an identity nothing else is meaningful: every row this device
    // writes is unattributable and loses every tie, so reporting the follow-on
    // issues for nine tables would bury the cause.
    //
    // Read from the database rather than the handle. `attach_readonly` builds a
    // handle with no identity on purpose — it must not initialize what it
    // inspects — so trusting the cached field would report every healthy library
    // as broken when checked from outside the app.
    if !stored_device_id_exists(db).await {
        issues.push(HealthIssue::DeviceIdMissing);
        return issues;
    }

    for table in SYNCED_TABLES {
        if !table_exists(db, table.name).await.unwrap_or(false) {
            issues.push(HealthIssue::MissingTable {
                table: table.name.to_string(),
            });
            continue;
        }

        let Ok(actual) = actual_columns(db, table.name).await else {
            continue;
        };

        let missing: Vec<String> = crate::sync_schema::SYNC_COLUMNS
            .iter()
            .filter(|c| !actual.iter().any(|a| a == *c))
            .map(|c| (*c).to_string())
            .collect();
        if !missing.is_empty() {
            // Nothing below can hold without the columns to hold it.
            issues.push(HealthIssue::MissingSyncColumns {
                table: table.name.to_string(),
                columns: missing,
            });
            continue;
        }

        for column in table.all_columns() {
            if !actual.iter().any(|a| a == column) {
                issues.push(HealthIssue::SyncSchemaDrift {
                    table: table.name.to_string(),
                    column: column.to_string(),
                });
            }
        }

        // Rows whose clock was never written. The columns are there and the
        // table reads as healthy; these rows simply cannot win a merge, which is
        // exactly the shape of the bug this module was written to catch.
        if let Ok(rows) = count_where(db, table.name, "updated_at = 0 OR updated_by = ''").await
            && rows > 0
        {
            issues.push(HealthIssue::UnstampedRows {
                table: table.name.to_string(),
                rows,
            });
        }

        if let Ok(rows) = count_where(db, table.name, "deleted = 1 AND updated_at = 0").await
            && rows > 0
        {
            issues.push(HealthIssue::TombstoneWithoutClock {
                table: table.name.to_string(),
                rows,
            });
        }
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

/// How many rows in `table` match `predicate`.
async fn count_where(db: &Database, table: &str, predicate: &str) -> Result<i64, turso::Error> {
    let mut rows = db
        .conn()
        .query(&format!("SELECT COUNT(*) FROM {table} WHERE {predicate}"), ())
        .await?;
    match rows.next().await? {
        Some(row) => Ok(row.get_value(0)?.as_integer().copied().unwrap_or(0)),
        None => Ok(0),
    }
}

/// Whether the library on disk records a device identity.
async fn stored_device_id_exists(db: &Database) -> bool {
    let Ok(mut rows) = db
        .conn()
        .query("SELECT site_id FROM crr_site_id LIMIT 1", ())
        .await
    else {
        return false;
    };
    matches!(rows.next().await, Ok(Some(_)))
}

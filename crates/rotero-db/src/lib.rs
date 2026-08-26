//! Database layer for Rotero, providing async CRUD operations over turso (pure Rust SQLite).
//!
//! Each submodule corresponds to a domain table (papers, annotations, collections, etc.)
//! and exposes standalone async functions that take a `&Connection`.

/// PDF annotation CRUD operations.
pub mod annotations;
/// One-time repair for libraries written without CRR change tracking.
mod backfill;
/// Collection (folder) CRUD and paper-collection membership.
pub mod collections;
/// Rotero's CRR (conflict-free replicated relations) schema configuration for sync.
pub mod crr;
/// Graph queries for paper-tag and paper-collection relationships.
pub mod graph;
/// Structural invariants an initialized database must satisfy.
pub mod health;
/// Per-paper note CRUD operations.
pub mod notes;
/// Paper CRUD, search, duplicate detection, and citation helpers.
pub mod papers;
/// Saved search CRUD operations.
pub mod saved_searches;
/// Table definitions, FTS index, and schema migrations.
pub mod schema;
/// Test utilities for simulating multi-device sync round-trips.
/// Stamping local writes so they can win a merge.
pub mod clock;

/// Serializing a device's synced tables, and merging a peer's.
pub mod snapshot;

/// Which tables and columns sync between devices.
pub mod sync_schema;

pub mod sync_test_helpers;
/// Tag CRUD and paper-tag membership.
pub mod tags;

pub use rotero_models::queries;

// Re-export so the app crate doesn't need a direct turso dependency.
pub use turso;

/// Errors from the rotero database layer.
///
/// Unifies the underlying turso driver errors and the `recrr` CRR-sync errors so
/// that both propagate through a single `?` in the db methods.
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    /// An error from the turso SQLite driver.
    #[error(transparent)]
    Turso(#[from] turso::Error),
    /// An error from the CRR change-tracking / sync engine.
    #[error(transparent)]
    Crr(#[from] recrr::Error),
}

/// Convenience alias for results in the db layer.
pub type DbResult<T> = Result<T, DbError>;

/// Trait for deserializing a turso Row into a domain model.
/// Each implementation maps column indices to struct fields based on the
/// SELECT column order in that model's query.
pub trait FromRow: Sized {
    fn from_row(row: &turso::Row) -> Self;
}

/// Collect all rows from an async turso Rows iterator into a Vec.
pub async fn collect_rows<T: FromRow>(rows: &mut turso::Rows) -> Result<Vec<T>, turso::Error> {
    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(T::from_row(&row));
    }
    Ok(out)
}

// --- Row reading helpers (used by FromRow impls) ---

/// Read a text column, returning empty string if NULL.
pub fn get_text(row: &turso::Row, idx: usize) -> String {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_text().cloned())
        .unwrap_or_default()
}

/// Read an optional text column.
pub fn get_opt_text(row: &turso::Row, idx: usize) -> Option<String> {
    row.get_value(idx).ok().and_then(|v| v.as_text().cloned())
}

/// Read an optional i64 column.
pub fn get_opt_i64(row: &turso::Row, idx: usize) -> Option<i64> {
    row.get_value(idx)
        .ok()
        .and_then(|v| v.as_integer().copied())
}

/// Read a boolean column (stored as 0/1 integer).
pub fn get_bool(row: &turso::Row, idx: usize) -> bool {
    get_opt_i64(row, idx).unwrap_or(0) != 0
}

// --- Value conversion helpers (used by insert/update functions) ---

/// Convert Option<&String> to Value::Text or Value::Null.
pub fn opt_text(opt: Option<&String>) -> turso::Value {
    opt.map(|s| turso::Value::Text(s.clone()))
        .unwrap_or(turso::Value::Null)
}

/// Convert Option<i64> to Value::Integer or Value::Null.
pub fn opt_int(opt: Option<i64>) -> turso::Value {
    opt.map(turso::Value::Integer).unwrap_or(turso::Value::Null)
}

use std::path::{Path, PathBuf};

use std::sync::Arc;

use recrr::backends::TursoDb;
use turso::Connection;

use crate::crr::{CrrStore, rotero_schema};

/// Handle to the Rotero SQLite database, wrapping a turso connection and the library data directory.
#[derive(Clone)]
pub struct Database {
    conn: Connection,
    data_dir: PathBuf,
    /// CRR change-tracking store, shared across clones of this handle.
    crr: Arc<CrrStore>,
    /// This device's sync identity, read once at open.
    ///
    /// Held rather than queried per write: it is the tiebreak half of every
    /// clock stamp, so it is read on a hot path and never changes for the life
    /// of the library.
    device_id: Arc<str>,
}

impl PartialEq for Database {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Database {
    /// Open (or create) the database at the given library directory.
    pub async fn open(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir: {e}"))?;

        let papers_dir = data_dir.join("papers");
        std::fs::create_dir_all(papers_dir.join("unsorted"))
            .map_err(|e| format!("Failed to create papers dir: {e}"))?;

        let db_path = data_dir.join("rotero.db");
        let db_path_str = db_path.to_string_lossy().to_string();

        let db = turso::Builder::new_local(&db_path_str)
            .experimental_index_method(true)
            .build()
            .await
            .map_err(|e| format!("Failed to open database: {e}"))?;

        let conn = db
            .connect()
            .map_err(|e| format!("Failed to connect: {e}"))?;

        schema::initialize_db(&conn)
            .await
            .map_err(|e| format!("Failed to initialize schema: {e}"))?;

        let crr = Arc::new(CrrStore::new(TursoDb::new(conn.clone()), rotero_schema()));

        // Whether this store's CRR metadata predates the current schema. On a
        // brand-new database there is no persisted fingerprint yet, so this is
        // false and `init` seeds every column (including `item_type`) fresh.
        let schema_drifted = matches!(
            crr.schema_fingerprint().await,
            Some(stored) if stored != crr.schema().fingerprint()
        );

        crr.init()
            .await
            .map_err(|e| format!("Failed to initialize CRR: {e}"))?;

        if schema_drifted {
            // An existing, already-synced store was compiled against a schema
            // that has since gained columns (`item_type` via the v11 SQL
            // migration, and anything added after it). Backfill clock entries so
            // existing rows emit those columns to peers.
            //
            // Every tracked column is offered rather than a hardcoded one: the
            // drift flag is a whole-schema fingerprint comparison, so naming a
            // single column here meant a second added column tripped the same
            // flag and was silently skipped. `migrate_add_column` is idempotent
            // and scoped to live rows, so offering a column that is already
            // backfilled costs one indexed query and changes nothing.
            for table in &crr.schema().tables {
                for column in &table.columns {
                    crr.migrate_add_column(&table.name, column)
                        .await
                        .map_err(|e| {
                            format!(
                                "Failed to backfill CRR clocks for {}.{column}: {e}",
                                table.name
                            )
                        })?;
                }
            }
        }

        let device_id = read_device_id(&conn).await?;

        let db = Self {
            conn,
            data_dir,
            crr,
            device_id,
        };

        // Adopt rows written by a build that never initialized CRR. Gated on a
        // persisted flag, so this is one indexed query per table on a single
        // launch and nothing afterwards. Non-fatal: a library that opens is more
        // useful than one that refuses to, and the health check reports the
        // underlying problem either way.
        if let Err(e) = db.backfill_untracked_rows().await {
            tracing::error!("CRR backfill failed: {e}");
        }

        Ok(db)
    }

    /// Open a library for inspection **without** initializing anything.
    ///
    /// Unlike [`Database::open`], this runs neither `initialize_db` nor
    /// `crr.init()`, so it reports the database exactly as it was left on disk.
    /// That is what a health check needs: opening normally would recreate any
    /// missing CRR metadata and mask the defect being looked for.
    ///
    /// Not for serving the app — writes through this handle may fail change
    /// tracking. Use [`Database::open`].
    pub async fn attach_readonly(data_dir: PathBuf) -> Result<Self, String> {
        let db_path = data_dir.join("rotero.db");
        let db = turso::Builder::new_local(&db_path.to_string_lossy())
            .experimental_index_method(true)
            .build()
            .await
            .map_err(|e| format!("Failed to open database: {e}"))?;
        let conn = db
            .connect()
            .map_err(|e| format!("Failed to connect: {e}"))?;
        Ok(Self::from_conn(conn, data_dir))
    }

    /// Wrap an existing connection without initializing anything.
    ///
    /// Private on purpose. Every public constructor either initializes the
    /// database ([`Database::open`]) or documents that it deliberately does not
    /// ([`Database::attach_readonly`]); an exposed version of this is what let a
    /// startup path build a `Database` whose CRR metadata was never created,
    /// committing rows that then failed change tracking.
    fn from_conn(conn: Connection, data_dir: PathBuf) -> Self {
        let crr = Arc::new(CrrStore::new(TursoDb::new(conn.clone()), rotero_schema()));
        Self {
            conn,
            data_dir,
            crr,
            // Left empty deliberately. This constructor backs `attach_readonly`,
            // which reports a database as-is without initializing it, so it must
            // not create an identity a health check is about to look for. The
            // health check reports the empty id rather than being handed one
            // this path invented.
            device_id: Arc::from(""),
        }
    }

    /// Rebuild a handle from parts taken from an existing, initialized database.
    ///
    /// Unlike [`from_conn`](Self::from_conn) this cannot skip initialization: the
    /// caller has to supply a [`CrrStore`] that already exists, and the only way
    /// to obtain one is [`crr_arc`](Self::crr_arc) on a database that was opened
    /// properly. That lets a wrapper around the same connection — the embedded
    /// MCP server — call shared write paths instead of reimplementing them, which
    /// is how its `delete_paper` drifted from the app's.
    pub fn from_parts(
        conn: Connection,
        data_dir: PathBuf,
        crr: Arc<CrrStore>,
        device_id: Arc<str>,
    ) -> Self {
        Self {
            conn,
            data_dir,
            crr,
            device_id,
        }
    }

    /// This device's identity, for handing to [`from_parts`](Self::from_parts).
    pub fn device_id_arc(&self) -> Arc<str> {
        Arc::clone(&self.device_id)
    }

    /// Returns a reference to the underlying turso connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// This device's sync identity, as lowercase hex.
    ///
    /// Written into `updated_by` on every local change, and used as the
    /// deterministic tiebreak when two devices stamp the same millisecond.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Returns the CRR change-tracking store for sync operations.
    pub fn crr(&self) -> &CrrStore {
        &self.crr
    }

    /// Shares this database's CRR store.
    ///
    /// For wrappers around the same connection (e.g. the embedded MCP server)
    /// that would otherwise construct a second, separately-initialized store.
    pub fn crr_arc(&self) -> Arc<CrrStore> {
        Arc::clone(&self.crr)
    }

    /// Returns the root library data directory (contains `rotero.db` and `papers/`).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Returns the directory where imported PDFs are stored.
    pub fn papers_dir(&self) -> PathBuf {
        self.data_dir.join("papers")
    }

    /// Resolve a relative PDF path to an absolute path within the papers directory.
    /// Guards against path traversal attacks.
    pub fn resolve_pdf_path(&self, rel_path: &str) -> PathBuf {
        let papers = self.papers_dir();
        let joined = papers.join(rel_path);
        // Prevent path traversal: if canonicalization shows the path escapes
        // papers_dir, return a path that won't resolve to anything sensitive.
        match (joined.canonicalize(), papers.canonicalize()) {
            (Ok(canonical), Ok(papers_canonical)) if canonical.starts_with(&papers_canonical) => {
                canonical
            }
            _ => {
                // File may not exist yet (pre-import), so also check logically
                // by normalizing away ".." components
                let mut normalized = papers.clone();
                for component in std::path::Path::new(rel_path).components() {
                    match component {
                        std::path::Component::Normal(c) => normalized.push(c),
                        std::path::Component::ParentDir
                            // Don't allow escaping papers dir
                            if normalized != papers => {
                                normalized.pop();
                            }
                        _ => {} // Skip CurDir, Prefix, RootDir
                    }
                }
                normalized
            }
        }
    }

    /// Import a PDF into the library.
    /// Layout: `papers/{year}/{Title} - {FirstAuthor}.pdf`, falling back to `papers/unsorted/`.
    pub fn import_pdf(
        &self,
        source_path: &str,
        title: Option<&str>,
        first_author: Option<&str>,
        year: Option<i32>,
    ) -> Result<String, String> {
        let source = Path::new(source_path);

        let clean_name = build_clean_filename(source, title, first_author);

        let subfolder = match year {
            Some(y) => y.to_string(),
            None => "unsorted".to_string(),
        };

        let rel_dir = Path::new(&subfolder);
        let abs_dir = self.papers_dir().join(rel_dir);
        std::fs::create_dir_all(&abs_dir).map_err(|e| format!("Failed to create folder: {e}"))?;

        let mut dest_name = clean_name.clone();
        let mut dest = abs_dir.join(&dest_name);
        let mut counter = 1;
        while dest.exists() {
            let stem = Path::new(&clean_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            dest_name = format!("{stem} ({counter}).pdf");
            dest = abs_dir.join(&dest_name);
            counter += 1;
        }

        std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy PDF: {e}"))?;

        let rel_path = std::path::Path::new(&subfolder)
            .join(&dest_name)
            .to_string_lossy()
            .into_owned();
        Ok(rel_path)
    }

    /// Import a PDF from bytes (e.g. downloaded from the web).
    /// Returns the relative path within the papers directory.
    pub fn import_pdf_bytes(
        &self,
        bytes: &[u8],
        title: &str,
        first_author: Option<&str>,
        year: Option<i32>,
    ) -> Result<String, String> {
        if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
            return Err("Not a valid PDF file".to_string());
        }

        let dummy_source = Path::new("download.pdf");
        let clean_name = build_clean_filename(dummy_source, Some(title), first_author);

        let subfolder = match year {
            Some(y) => y.to_string(),
            None => "unsorted".to_string(),
        };

        let rel_dir = Path::new(&subfolder);
        let abs_dir = self.papers_dir().join(rel_dir);
        std::fs::create_dir_all(&abs_dir).map_err(|e| format!("Failed to create folder: {e}"))?;

        let mut dest_name = clean_name.clone();
        let mut dest = abs_dir.join(&dest_name);
        let mut counter = 1;
        while dest.exists() {
            let stem = Path::new(&clean_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            dest_name = format!("{stem} ({counter}).pdf");
            dest = abs_dir.join(&dest_name);
            counter += 1;
        }

        std::fs::write(&dest, bytes).map_err(|e| format!("Failed to write PDF: {e}"))?;

        let rel_path = std::path::Path::new(&subfolder)
            .join(&dest_name)
            .to_string_lossy()
            .into_owned();
        Ok(rel_path)
    }
}

/// Format: "Title - Author.pdf", falling back to original filename.
fn build_clean_filename(source: &Path, title: Option<&str>, first_author: Option<&str>) -> String {
    let original = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "paper".to_string());

    match (title, first_author) {
        (Some(t), Some(a)) => {
            let clean_title = sanitize_filename(t, 80);
            let clean_author = sanitize_filename(a, 40);
            format!("{clean_title} - {clean_author}.pdf")
        }
        (Some(t), None) => {
            let clean_title = sanitize_filename(t, 100);
            format!("{clean_title}.pdf")
        }
        _ => {
            // Fall back to original filename
            let clean = sanitize_filename(&original, 100);
            format!("{clean}.pdf")
        }
    }
}

/// Remove filesystem-unsafe characters and truncate to `max_len`.
fn sanitize_filename(s: &str, max_len: usize) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            '\n' | '\r' | '\t' => ' ',
            _ => c,
        })
        .collect();

    let trimmed = cleaned.trim();

    // By character: filenames are built from paper titles, so a byte cut at
    // `max_len` panicked on any title with a multi-byte character near it.
    let truncated = rotero_models::take_chars(trimmed, max_len);
    if truncated.len() == trimmed.len() {
        return truncated;
    }

    // Prefer a word boundary, keeping at least half the budget.
    match truncated.rfind(' ') {
        Some(pos) if pos > truncated.len() / 2 => truncated[..pos].to_string(),
        _ => truncated,
    }
}

/// Read this device's sync identity, creating one if the library has none.
///
/// The table is created by Rotero's own migration rather than by recrr, so the
/// identity survives that dependency being removed — which matters, because a
/// device that changed id would look to every peer like a brand-new one and
/// re-send its whole library.
async fn read_device_id(conn: &Connection) -> Result<Arc<str>, String> {
    let _ = conn
        .execute(
            "INSERT OR IGNORE INTO crr_site_id (site_id) VALUES (randomblob(16))",
            (),
        )
        .await;

    let mut rows = conn
        .query("SELECT lower(hex(site_id)) FROM crr_site_id LIMIT 1", ())
        .await
        .map_err(|e| format!("Failed to read device id: {e}"))?;

    let id = rows
        .next()
        .await
        .map_err(|e| format!("Failed to read device id: {e}"))?
        .and_then(|r| r.get_value(0).ok())
        .and_then(|v| v.as_text().cloned())
        .ok_or_else(|| "Failed to read device id: no row".to_string())?;

    Ok(Arc::from(id.as_str()))
}

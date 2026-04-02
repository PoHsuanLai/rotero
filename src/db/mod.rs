pub mod schema;
pub mod papers;
pub mod collections;
pub mod tags;
pub mod annotations;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use directories::ProjectDirs;
use rusqlite::Connection;

/// Thread-safe wrapper around a SQLite connection.
#[derive(Clone)]
pub struct Database {
    conn: Arc<Mutex<Connection>>,
    data_dir: PathBuf,
}

impl PartialEq for Database {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.conn, &other.conn)
    }
}

impl Database {
    /// Initialize the database in the platform-appropriate data directory.
    pub fn init() -> Result<Self, String> {
        let dirs = ProjectDirs::from("com", "rotero", "Rotero")
            .ok_or("Could not determine data directory")?;

        let data_dir = dirs.data_dir().to_path_buf();
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir: {e}"))?;

        // Create pdfs subdirectory for storing imported PDFs
        let pdfs_dir = data_dir.join("pdfs");
        std::fs::create_dir_all(&pdfs_dir)
            .map_err(|e| format!("Failed to create pdfs dir: {e}"))?;

        let db_path = data_dir.join("rotero.db");
        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {e}"))?;

        schema::initialize_db(&conn)
            .map_err(|e| format!("Failed to initialize schema: {e}"))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            data_dir,
        })
    }

    /// Run a closure with the database connection.
    pub fn with_conn<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&Connection) -> rusqlite::Result<R>,
    {
        let conn = self.conn.lock().map_err(|e| format!("DB lock error: {e}"))?;
        f(&conn).map_err(|e| format!("DB error: {e}"))
    }

    /// Path to the pdfs storage directory.
    pub fn pdfs_dir(&self) -> PathBuf {
        self.data_dir.join("pdfs")
    }

    /// Copy a PDF into the managed pdfs directory, return the relative path.
    pub fn import_pdf(&self, source_path: &str) -> Result<String, String> {
        let source = std::path::Path::new(source_path);
        let filename = source
            .file_name()
            .ok_or("Invalid source path")?
            .to_string_lossy()
            .to_string();

        // Add timestamp prefix to avoid collisions
        let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let dest_name = format!("{ts}_{filename}");
        let dest = self.pdfs_dir().join(&dest_name);

        std::fs::copy(source, &dest)
            .map_err(|e| format!("Failed to copy PDF: {e}"))?;

        Ok(dest_name)
    }
}

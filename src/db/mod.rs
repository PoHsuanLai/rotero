pub mod schema;
pub mod papers;
pub mod collections;
pub mod tags;
pub mod annotations;

use std::path::PathBuf;

use turso::Connection;

use crate::sync::engine::SyncConfig;

/// Wrapper around turso Database + Connection.
/// Connection is Clone + Send + Sync, so no Arc<Mutex<>> needed.
#[derive(Clone)]
pub struct Database {
    conn: Connection,
    data_dir: PathBuf,
}

impl PartialEq for Database {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Database {
    /// Initialize the database using the sync config for path resolution.
    pub async fn init() -> Result<Self, String> {
        let config = SyncConfig::load();
        let data_dir = config.effective_library_path();

        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir: {e}"))?;

        let pdfs_dir = data_dir.join("pdfs");
        std::fs::create_dir_all(&pdfs_dir)
            .map_err(|e| format!("Failed to create pdfs dir: {e}"))?;

        let db_path = data_dir.join("rotero.db");
        let db_path_str = db_path.to_string_lossy().to_string();

        let db = turso::Builder::new_local(&db_path_str)
            .experimental_index_method(true)
            .build()
            .await
            .map_err(|e| format!("Failed to open database: {e}"))?;

        let conn = db.connect().map_err(|e| format!("Failed to connect: {e}"))?;

        schema::initialize_db(&conn)
            .await
            .map_err(|e| format!("Failed to initialize schema: {e}"))?;

        Ok(Self { conn, data_dir })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn pdfs_dir(&self) -> PathBuf {
        self.data_dir.join("pdfs")
    }

    pub fn import_pdf(&self, source_path: &str) -> Result<String, String> {
        let source = std::path::Path::new(source_path);
        let filename = source
            .file_name()
            .ok_or("Invalid source path")?
            .to_string_lossy()
            .to_string();

        let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
        let dest_name = format!("{ts}_{filename}");
        let dest = self.pdfs_dir().join(&dest_name);

        std::fs::copy(source, &dest)
            .map_err(|e| format!("Failed to copy PDF: {e}"))?;

        Ok(dest_name)
    }
}

use std::path::Path;

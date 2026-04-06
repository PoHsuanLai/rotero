/// Shared DB connection — initialized once, used by both connector and UI.
#[cfg(feature = "desktop")]
pub static SHARED_DB: std::sync::OnceLock<(rotero_db::turso::Connection, std::path::PathBuf)> =
    std::sync::OnceLock::new();

/// Initialize the shared database connection and run migrations.
#[cfg(feature = "desktop")]
pub(crate) fn init_database(config: &crate::sync::engine::SyncConfig) {
    let lib_path = config.effective_library_path();
    std::fs::create_dir_all(&lib_path).expect("Failed to create library directory");
    let papers_dir = lib_path.join("papers");
    let _ = std::fs::create_dir_all(papers_dir.join("unsorted"));
    let db_path = lib_path.join("rotero.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    let rt = tokio::runtime::Runtime::new().expect("Failed to create init runtime");
    let (conn, _lib_path) = rt.block_on(async {
        let db = rotero_db::turso::Builder::new_local(&db_path_str)
            .experimental_index_method(true)
            .build()
            .await
            .expect("Failed to open database");
        let conn = db.connect().expect("Failed to connect to database");
        rotero_db::schema::initialize_db(&conn)
            .await
            .expect("Failed to initialize schema");
        (conn, lib_path.clone())
    });
    let _ = SHARED_DB.set((conn, lib_path));
}

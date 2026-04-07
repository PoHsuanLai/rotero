#[cfg(feature = "desktop")]
pub static SHARED_DB: std::sync::OnceLock<(rotero_db::turso::Connection, std::path::PathBuf)> =
    std::sync::OnceLock::new();

#[cfg(feature = "desktop")]
pub(crate) fn init_database(config: &crate::sync::engine::SyncConfig) -> Result<(), String> {
    let lib_path = config.effective_library_path();
    std::fs::create_dir_all(&lib_path)
        .map_err(|e| format!("Failed to create library directory: {e}"))?;
    let papers_dir = lib_path.join("papers");
    let _ = std::fs::create_dir_all(papers_dir.join("unsorted"));
    let db_path = lib_path.join("rotero.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| format!("Failed to create init runtime: {e}"))?;
    let (conn, _lib_path) = rt.block_on(async {
        let db = rotero_db::turso::Builder::new_local(&db_path_str)
            .experimental_index_method(true)
            .build()
            .await
            .map_err(|e| format!("Failed to open database: {e}"))?;
        let conn = db
            .connect()
            .map_err(|e| format!("Failed to connect to database: {e}"))?;
        rotero_db::schema::initialize_db(&conn)
            .await
            .map_err(|e| format!("Failed to initialize schema: {e}"))?;
        Ok::<_, String>((conn, lib_path.clone()))
    })?;
    let _ = SHARED_DB.set((conn, lib_path));
    Ok(())
}

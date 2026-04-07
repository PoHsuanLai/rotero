pub(crate) fn init_logging() {
    let log_path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("rotero-debug.log");
    match std::fs::File::create(&log_path) {
        Ok(log_file) => {
            let _ = tracing_subscriber::fmt()
                .with_writer(std::sync::Mutex::new(log_file))
                .with_env_filter(
                    tracing_subscriber::EnvFilter::builder()
                        .parse("warn,rotero=debug,rotero_pdf=debug")
                        .unwrap(),
                )
                .try_init();
            tracing::info!("Logging to {}", log_path.display());
        }
        Err(e) => {
            // Fallback to stderr if log file creation fails
            let _ = tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_env_filter(
                    tracing_subscriber::EnvFilter::builder()
                        .parse("warn,rotero=debug,rotero_pdf=debug")
                        .unwrap(),
                )
                .try_init();
            tracing::warn!(
                "Failed to create log file at {}: {e}, logging to stderr",
                log_path.display()
            );
        }
    }
}

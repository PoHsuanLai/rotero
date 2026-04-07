pub(crate) fn init_logging() {
    let log_path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("rotero-debug.log");
    let log_file = std::fs::File::create(&log_path).expect("Failed to create log file");
    let _ = tracing_subscriber::fmt()
        .with_writer(std::sync::Mutex::new(log_file))
        .with_env_filter(
            tracing_subscriber::EnvFilter::builder()
                .parse("warn,rotero=debug,rotero_pdf=debug")
                .unwrap(),
        )
        .try_init();
    eprintln!("Logging to {}", log_path.display());
}

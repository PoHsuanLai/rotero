/// The default verbosity when `RUST_LOG` says nothing.
const DEFAULT_FILTER: &str = "warn,rotero=info,rotero_pdf=info";

pub(crate) fn init_logging() {
    // `RUST_LOG` was ignored entirely, so the log was pinned at debug in release
    // builds with no way to turn it down. The default is also `info` rather than
    // `debug` now: debug logging of a desktop app's whole session writes a lot
    // about what the user was doing, and anything genuinely needed for a bug
    // report can be turned back on with `RUST_LOG=rotero=debug`.
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(DEFAULT_FILTER))
    };

    let log_path = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("rotero-debug.log");
    match std::fs::File::create(&log_path) {
        Ok(log_file) => {
            // The log records library activity and scraped hosts, so it should
            // not be readable by every account on the machine.
            crate::sync::engine::restrict_permissions(&log_path);
            let _ = tracing_subscriber::fmt()
                .with_writer(std::sync::Mutex::new(log_file))
                .with_env_filter(filter())
                .try_init();
            tracing::info!("Logging to {}", log_path.display());
        }
        Err(e) => {
            // Fallback to stderr if log file creation fails
            let _ = tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .with_env_filter(filter())
                .try_init();
            tracing::warn!(
                "Failed to create log file at {}: {e}, logging to stderr",
                log_path.display()
            );
        }
    }
}

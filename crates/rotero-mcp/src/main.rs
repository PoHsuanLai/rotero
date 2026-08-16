mod db;
mod server;

use std::path::PathBuf;

use clap::Parser;
use rmcp::ServiceExt;
use rmcp::transport::io::stdio;

#[derive(Parser)]
#[command(name = "rotero-mcp", about = "Rotero paper library MCP server")]
struct Cli {
    /// Path to the Rotero SQLite database file.
    /// Defaults to the standard Rotero data directory.
    #[arg(long)]
    db_path: Option<PathBuf>,
}

fn default_db_path() -> PathBuf {
    // A server that exits with a panic message is harder to diagnose than one
    // that reports a path it could not resolve, so fall back and let the open
    // fail with something actionable. `ROTERO_DB_PATH` overrides this anyway.
    match directories::ProjectDirs::from("com", "rotero", "Rotero") {
        Some(dirs) => dirs.data_dir().join("rotero.db"),
        None => {
            tracing::warn!(
                "Could not determine the platform data directory; \
                 set ROTERO_DB_PATH to point at your library"
            );
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".rotero").join("rotero.db")
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log to stderr (stdout is reserved for JSON-RPC protocol)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();

    let db_path = cli
        .db_path
        .or_else(|| std::env::var("ROTERO_DB_PATH").ok().map(PathBuf::from))
        .unwrap_or_else(default_db_path);

    tracing::info!("Opening database at {}", db_path.display());

    let db = db::Database::open(&db_path)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    // Check if pdfium is available (probe only, engine created per-call on blocking thread)
    let pdf_available = match rotero_pdf::PdfEngine::new(None) {
        Ok(_) => {
            tracing::info!("PDF engine available");
            true
        }
        Err(e) => {
            tracing::warn!("PDF engine not available: {e}. PDF text extraction will be disabled.");
            false
        }
    };

    let server = server::RoteroMcp::new(db, pdf_available);

    tracing::info!("Starting Rotero MCP server");

    let service = server.serve(stdio()).await.inspect_err(|e| {
        tracing::error!("Server error: {e:?}");
    })?;

    service.waiting().await?;

    Ok(())
}

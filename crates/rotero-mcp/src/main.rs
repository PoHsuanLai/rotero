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
    let dirs = directories::ProjectDirs::from("com", "rotero", "Rotero")
        .expect("Could not determine data directory");
    dirs.data_dir().join("rotero.db")
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

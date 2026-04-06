//! Main server setup, router, and re-exports.

pub mod params;
mod tools;

use rmcp::handler::server::tool::ToolRouter;

use crate::db::Database;

pub use params::*;

#[derive(Clone)]
pub struct RoteroMcp {
    db: Database,
    /// Whether pdfium is available (checked at startup).
    pdf_available: bool,
    tool_router: ToolRouter<Self>,
}


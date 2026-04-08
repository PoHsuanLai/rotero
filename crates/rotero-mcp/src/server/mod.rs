//! Main server setup, router, and re-exports.

pub mod params;
mod tools;

use rmcp::handler::server::tool::ToolRouter;

use crate::db::Database;

/// MCP server that exposes the Rotero paper library via tools, resources, and prompts.
#[derive(Clone)]
pub struct RoteroMcp {
    db: Database,
    /// Whether pdfium is available (checked at startup).
    #[allow(dead_code)] // stored for future use gating PDF tools
    pdf_available: bool,
    tool_router: ToolRouter<Self>,
}

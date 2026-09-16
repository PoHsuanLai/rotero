//! Main server setup, router, and re-exports.

pub mod params;
mod pdf;
mod tools;

use crate::db::Database;

/// MCP server that exposes the Rotero paper library via tools, resources, and prompts.
#[derive(Clone)]
pub struct RoteroMcp {
    db: Database,
}

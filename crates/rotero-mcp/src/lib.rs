//! Model Context Protocol (MCP) server for Rotero. Exposes the paper library
//! to AI assistants via tools, resources, and prompts.

/// Read-only database access layer for the MCP server.
pub mod db;
/// MCP server implementation, tool routing, and parameter types.
pub mod server;

/// Handle to the Rotero SQLite database.
pub use db::{Database, OnChangeFn};
/// The MCP server handler that wires tools, resources, and prompts.
pub use server::RoteroMcp;

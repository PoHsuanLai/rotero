//! Table definitions and migrations.

/// Schema migration logic and version tracking.
pub mod migrations;
/// SQL CREATE TABLE and FTS index statements.
pub mod tables;

pub use migrations::initialize_db;

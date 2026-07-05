//! The JS-side sandbox shim, injected before a translator runs. It defines the
//! `Zotero`, `ZU`/`Zotero.Utilities`, and `doc` globals in terms of the Rust
//! host functions. The JavaScript lives in `sandbox.js` (embedded at build time
//! via `include_str!`) so it keeps JS syntax highlighting and stays out of the
//! Rust source.

/// The sandbox shim JavaScript, evaluated before each translator runs.
pub const SHIM: &str = include_str!("sandbox.js");

//! Rust ports of Zotero `Zotero.Utilities` (ZU) functions where correctness is
//! most important: author-name splitting (with surname particles) and date
//! parsing.
//!
//! Native translators call these directly; the JS engine exposes them as host
//! functions so both share one implementation.

mod author;
mod date;

pub use author::{CleanedAuthor, clean_author};
pub use date::{ParsedDate, str_to_date};

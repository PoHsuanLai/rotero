//! Pure-Rust ports of the load-bearing Zotero `Zotero.Utilities` (ZU) functions
//! — the ones whose correctness matters most across translators: author-name
//! splitting (with surname particles) and date parsing.
//!
//! These are used by native translators directly and exposed to the JS engine
//! as host functions so it can share one implementation. Kept deliberately
//! close to the upstream behavior; each has focused unit tests.

mod author;
mod date;

pub use author::{CleanedAuthor, clean_author};
pub use date::{ParsedDate, str_to_date};

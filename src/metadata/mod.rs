pub mod doi_extract;
pub mod enrich;

// Re-export from rotero-search crate
pub use rotero_search::arxiv;
pub use rotero_search::crossref;
pub use rotero_search::openalex;
pub use rotero_search::semantic_scholar;
pub use rotero_search::unpaywall;

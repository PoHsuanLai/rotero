//! Shared data types for the Rotero paper reading app.
//!
//! Provides model structs (Paper, Annotation, Collection, Note, Tag, SavedSearch)
//! and reusable SQL query constants used across the application.

/// PDF annotation types and data.
pub mod annotation;
/// Hierarchical folder-like groupings for papers.
pub mod collection;
/// Free-form notes attached to papers.
pub mod note;
/// Core paper metadata and helper methods.
pub mod paper;
/// Reusable SQL query constants for all tables.
pub mod queries;
/// Persisted search queries.
pub mod saved_search;
/// User-defined labels for papers.
pub mod tag;

pub use annotation::{Annotation, AnnotationType};
pub use collection::Collection;
pub use note::Note;
pub use paper::{CitationInfo, LibraryStatus, Paper, PaperId, PaperLinks, Publication};
pub use saved_search::SavedSearch;
pub use tag::Tag;

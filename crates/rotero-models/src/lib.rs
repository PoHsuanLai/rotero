//! Shared data types for the Rotero paper reading app.
//!
//! Provides model structs (Paper, Annotation, Collection, Note, Tag, SavedSearch)
//! and reusable SQL query constants used across the application.

/// PDF annotation types and data.
pub mod annotation;
/// Hierarchical folder-like groupings for papers.
pub mod collection;
/// Merge, dedup, and rank web-search results across providers.
pub mod merge;
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
pub use collection::{Collection, children_of, collection_tree, has_children};
pub use merge::merge_and_rank;
pub use note::Note;
pub use paper::{
    CitationInfo, Creator, CreatorRole, LibraryStatus, Paper, PaperId, PaperLinks, ProviderKind,
    Publication, SearchRank, build_fts_match_query, local_relevance_score, normalize_title,
    rank_local_results,
};
pub use saved_search::SavedSearch;
pub use tag::Tag;

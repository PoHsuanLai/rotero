//! Metadata search clients for academic paper discovery.
//!
//! Provides unified access to arXiv, CrossRef, OpenAlex, Semantic Scholar,
//! and Unpaywall APIs for fetching paper metadata by DOI, title, or keyword.

/// arXiv API client for searching and fetching preprint metadata.
pub mod arxiv;
/// CrossRef API client for DOI-based metadata lookup.
pub mod crossref;
/// OpenAlex API client for search, autocomplete, and open-access PDF discovery.
pub mod openalex;
/// Semantic Scholar API client for paper search and citation data.
pub mod semantic_scholar;
/// Unpaywall API client for finding open-access PDF URLs.
pub mod unpaywall;

use std::sync::OnceLock;

use rotero_models::Paper;

/// Shared HTTP client — reuses connections and TLS sessions across all API calls.
static SHARED_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Returns the shared HTTP client, initializing it on first call.
pub fn shared_client() -> &'static reqwest::Client {
    SHARED_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("Rotero/0.1.0 (mailto:rotero@example.com)")
            .build()
            .expect("Failed to build HTTP client")
    })
}

/// Search provider enum — replaces trait with direct async dispatch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchProvider {
    OpenAlex,
    ArXiv,
    SemanticScholar,
}

impl SearchProvider {
    /// Returns the human-readable display name for this provider.
    pub fn name(&self) -> &'static str {
        match self {
            Self::OpenAlex => "OpenAlex",
            Self::ArXiv => "arXiv",
            Self::SemanticScholar => "Semantic Scholar",
        }
    }

    /// Fast search for live type-ahead. OpenAlex uses autocomplete endpoint.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        match self {
            Self::OpenAlex => openalex::autocomplete(query).await,
            Self::ArXiv => arxiv::search_papers(query, limit).await,
            Self::SemanticScholar => semantic_scholar::search_papers(query, limit).await,
        }
    }

    /// Full search with complete metadata (authors, abstract, etc.).
    /// For OpenAlex this uses the slower but richer search endpoint.
    pub async fn search_full(&self, query: &str, limit: usize) -> Result<Vec<Paper>, String> {
        match self {
            Self::OpenAlex => openalex::search_papers(query, limit).await,
            Self::ArXiv => arxiv::search_papers(query, limit).await,
            Self::SemanticScholar => semantic_scholar::search_papers(query, limit).await,
        }
    }

    /// Fetches a single paper by DOI or other identifier string.
    /// The raw string is parsed through [`PaperId`](rotero_models::PaperId) for
    /// correct routing (e.g. arXiv DOIs go to the arXiv endpoint).
    pub async fn fetch_by_doi(&self, doi: &str) -> Result<Paper, String> {
        let id = rotero_models::PaperId::parse(doi);
        match self {
            Self::OpenAlex => openalex::fetch_by_doi(doi).await,
            Self::ArXiv => {
                let arxiv_id = match &id {
                    Some(rotero_models::PaperId::ArXiv(a)) => a.as_str(),
                    _ => doi,
                };
                arxiv::fetch_by_arxiv_id(arxiv_id).await
            }
            Self::SemanticScholar => semantic_scholar::fetch_by_doi(doi).await,
        }
    }

    /// Whether this provider returns sparse results from `search()` that
    /// should be enriched with a follow-up `search_full()` call.
    pub fn needs_enrichment(&self) -> bool {
        matches!(self, Self::OpenAlex)
    }
}

/// All available search providers, in default display order.
pub static ALL_PROVIDERS: &[SearchProvider] = &[
    SearchProvider::OpenAlex,
    SearchProvider::ArXiv,
    SearchProvider::SemanticScholar,
];

pub mod arxiv;
pub mod crossref;
pub mod openalex;
pub mod parser;
pub mod semantic_scholar;
pub mod unpaywall;

use std::sync::OnceLock;

/// Shared HTTP client — reuses connections and TLS sessions across all API calls.
static SHARED_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn shared_client() -> &'static reqwest::Client {
    SHARED_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("Rotero/0.1.0 (mailto:rotero@example.com)")
            .build()
            .expect("Failed to build HTTP client")
    })
}

/// Metadata fetched from an external API.
pub struct FetchedMetadata {
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub abstract_text: Option<String>,
    pub url: Option<String>,
    pub doi: String,
    pub citation_count: Option<i64>,
}

/// Search provider enum — replaces trait with direct async dispatch.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SearchProvider {
    OpenAlex,
    ArXiv,
    SemanticScholar,
}

impl SearchProvider {
    pub fn name(&self) -> &'static str {
        match self {
            Self::OpenAlex => "OpenAlex",
            Self::ArXiv => "arXiv",
            Self::SemanticScholar => "Semantic Scholar",
        }
    }

    /// Fast search for live type-ahead. OpenAlex uses autocomplete endpoint.
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<FetchedMetadata>, String> {
        match self {
            Self::OpenAlex => openalex::autocomplete(query).await,
            Self::ArXiv => arxiv::search_papers(query, limit).await,
            Self::SemanticScholar => semantic_scholar::search_papers(query, limit).await,
        }
    }

    /// Full search with complete metadata (authors, abstract, etc.).
    /// For OpenAlex this uses the slower but richer search endpoint.
    pub async fn search_full(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FetchedMetadata>, String> {
        match self {
            Self::OpenAlex => openalex::search_papers(query, limit).await,
            Self::ArXiv => arxiv::search_papers(query, limit).await,
            Self::SemanticScholar => semantic_scholar::search_papers(query, limit).await,
        }
    }

    /// Fetch full metadata by DOI.
    pub async fn fetch_by_doi(&self, doi: &str) -> Result<FetchedMetadata, String> {
        match self {
            Self::OpenAlex => openalex::fetch_by_doi(doi).await,
            Self::ArXiv => {
                let id = doi.strip_prefix("arXiv:").unwrap_or(doi);
                arxiv::fetch_by_arxiv_id(id).await
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

pub static ALL_PROVIDERS: &[SearchProvider] = &[
    SearchProvider::OpenAlex,
    SearchProvider::ArXiv,
    SearchProvider::SemanticScholar,
];

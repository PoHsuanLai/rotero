pub mod arxiv;
pub mod crossref;
pub mod openalex;
pub mod parser;
pub mod semantic_scholar;
pub mod unpaywall;

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

/// Trait for paper search providers (OpenAlex, arXiv, Semantic Scholar, etc.)
pub trait PaperSearchProvider: Send + Sync {
    fn name(&self) -> &'static str;

    /// Fast search for live type-ahead. May return lightweight results.
    fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<FetchedMetadata>, String>> + Send>,
    >;

    /// Fetch full metadata by DOI. Used to enrich sparse results on import.
    fn fetch_by_doi(
        &self,
        doi: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FetchedMetadata, String>> + Send>>;
}

pub struct OpenAlexProvider;
pub struct ArXivProvider;
pub struct SemanticScholarProvider;

impl PaperSearchProvider for OpenAlexProvider {
    fn name(&self) -> &'static str {
        "OpenAlex"
    }

    fn search(
        &self,
        query: &str,
        _limit: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<FetchedMetadata>, String>> + Send>,
    > {
        let query = query.to_string();
        Box::pin(async move { openalex::autocomplete(&query).await })
    }

    fn fetch_by_doi(
        &self,
        doi: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FetchedMetadata, String>> + Send>>
    {
        let doi = doi.to_string();
        Box::pin(async move { openalex::fetch_by_doi(&doi).await })
    }
}

impl PaperSearchProvider for ArXivProvider {
    fn name(&self) -> &'static str {
        "arXiv"
    }

    fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<FetchedMetadata>, String>> + Send>,
    > {
        let query = query.to_string();
        Box::pin(async move { arxiv::search_papers(&query, limit).await })
    }

    fn fetch_by_doi(
        &self,
        doi: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FetchedMetadata, String>> + Send>>
    {
        let id = doi.strip_prefix("arXiv:").unwrap_or(doi).to_string();
        Box::pin(async move { arxiv::fetch_by_arxiv_id(&id).await })
    }
}

impl PaperSearchProvider for SemanticScholarProvider {
    fn name(&self) -> &'static str {
        "Semantic Scholar"
    }

    fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Vec<FetchedMetadata>, String>> + Send>,
    > {
        let query = query.to_string();
        Box::pin(async move { semantic_scholar::search_papers(&query, limit).await })
    }

    fn fetch_by_doi(
        &self,
        doi: &str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<FetchedMetadata, String>> + Send>>
    {
        let doi = doi.to_string();
        Box::pin(async move { semantic_scholar::fetch_by_doi(&doi).await })
    }
}

/// All available search providers.
pub static PROVIDERS: &[&dyn PaperSearchProvider] =
    &[&OpenAlexProvider, &ArXivProvider, &SemanticScholarProvider];

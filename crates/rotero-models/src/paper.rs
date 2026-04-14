use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Normalized academic paper identifier.
///
/// Provides a single source of truth for parsing, storing, and routing
/// identifier strings that live in the `Paper.doi` field.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaperId {
    /// Standard DOI, e.g. `"10.1038/nature12373"`.
    Doi(String),
    /// arXiv ID (bare, no prefix), e.g. `"2401.11660"`.
    ArXiv(String),
    /// PubMed ID, e.g. `"12345678"`.
    Pmid(String),
    /// ISBN, e.g. `"978-0-123456-47-2"`.
    Isbn(String),
}

impl PaperId {
    /// Parse a raw identifier string into a typed `PaperId`.
    ///
    /// Recognizes these formats:
    /// - `"arXiv:2401.11660"` → `ArXiv("2401.11660")`
    /// - `"10.48550/arXiv.2401.11660"` → `ArXiv("2401.11660")`
    /// - `"10.1038/nature12373"` → `Doi("10.1038/nature12373")`
    /// - `"PMID:12345678"` → `Pmid("12345678")`
    /// - `"ISBN:978-0-123456-47-2"` → `Isbn("978-0-123456-47-2")`
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }

        // arXiv with explicit prefix (our stored format)
        if let Some(id) = raw.strip_prefix("arXiv:") {
            return Some(Self::ArXiv(id.to_string()));
        }

        // arXiv DOI (10.48550/arXiv.XXXX.XXXXX)
        if let Some(id) = raw
            .strip_prefix("10.48550/arXiv.")
            .or_else(|| raw.strip_prefix("10.48550/arxiv."))
        {
            return Some(Self::ArXiv(id.to_string()));
        }

        // PMID
        if let Some(id) = raw.strip_prefix("PMID:").or_else(|| raw.strip_prefix("pmid:")) {
            let id = id.trim();
            if !id.is_empty() {
                return Some(Self::Pmid(id.to_string()));
            }
        }

        // ISBN
        if let Some(id) = raw.strip_prefix("ISBN:").or_else(|| raw.strip_prefix("isbn:")) {
            let id = id.trim();
            if !id.is_empty() {
                return Some(Self::Isbn(id.to_string()));
            }
        }

        // Standard DOI (starts with "10.")
        if raw.starts_with("10.") {
            return Some(Self::Doi(raw.to_string()));
        }

        None
    }

    /// Serialize to the backward-compatible string format stored in the DB.
    pub fn to_stored_string(&self) -> String {
        match self {
            Self::Doi(d) => d.clone(),
            Self::ArXiv(id) => format!("arXiv:{id}"),
            Self::Pmid(id) => format!("PMID:{id}"),
            Self::Isbn(id) => format!("ISBN:{id}"),
        }
    }

    /// Returns the Semantic Scholar API path segment for this identifier.
    pub fn semantic_scholar_query(&self) -> String {
        match self {
            Self::Doi(d) => format!("DOI:{d}"),
            Self::ArXiv(id) => format!("ARXIV:{id}"),
            Self::Pmid(id) => format!("PMID:{id}"),
            Self::Isbn(_) => String::new(), // S2 doesn't support ISBN lookup
        }
    }

    /// Returns `true` if this is an arXiv identifier.
    pub fn is_arxiv(&self) -> bool {
        matches!(self, Self::ArXiv(_))
    }
}

impl fmt::Display for PaperId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Doi(d) => write!(f, "{d}"),
            Self::ArXiv(id) => write!(f, "arXiv:{id}"),
            Self::Pmid(id) => write!(f, "PMID:{id}"),
            Self::Isbn(id) => write!(f, "ISBN:{id}"),
        }
    }
}

/// Publication venue metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Publication {
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
}

/// URLs and local file paths for a paper.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PaperLinks {
    pub url: Option<String>,
    pub pdf_url: Option<String>,
    pub pdf_path: Option<String>,
}

/// Library-level status fields (dates, read/favorite flags).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryStatus {
    pub date_added: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
    pub is_favorite: bool,
    pub is_read: bool,
}

impl Default for LibraryStatus {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            date_added: now,
            date_modified: now,
            is_favorite: false,
            is_read: false,
        }
    }
}

/// Citation key, count, and extra metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CitationInfo {
    pub citation_count: Option<i64>,
    pub citation_key: Option<String>,
    pub extra_meta: Option<serde_json::Value>,
}

/// A research paper with full metadata, links, library status, and citation info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Paper {
    pub id: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub abstract_text: Option<String>,
    pub publication: Publication,
    pub links: PaperLinks,
    pub status: LibraryStatus,
    pub citation: CitationInfo,
}

impl Paper {
    /// Create a new paper with the given title and default values for all other fields.
    pub fn new(title: String) -> Self {
        Self {
            title,
            ..Default::default()
        }
    }

    /// Format authors for display: "Unknown", "A, B", or "A et al."
    pub fn formatted_authors(&self) -> String {
        if self.authors.is_empty() {
            "Unknown".to_string()
        } else if self.authors.len() <= 2 {
            self.authors.join(", ")
        } else {
            format!("{} et al.", self.authors[0])
        }
    }

    /// Parse the `doi` field into a typed [`PaperId`], if present and recognized.
    pub fn paper_id(&self) -> Option<PaperId> {
        self.doi.as_deref().and_then(PaperId::parse)
    }

    /// Score how complete the metadata is (higher = more complete).
    /// PDF presence is weighted higher since it's the primary asset.
    pub fn metadata_completeness_score(&self) -> i32 {
        let mut c = 0i32;
        if self.doi.is_some() {
            c += 1;
        }
        if self.abstract_text.is_some() {
            c += 1;
        }
        if self.publication.journal.is_some() {
            c += 1;
        }
        if self.year.is_some() {
            c += 1;
        }
        if self.links.pdf_path.is_some() {
            c += 2;
        }
        if !self.authors.is_empty() {
            c += 1;
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_standard_doi() {
        assert_eq!(
            PaperId::parse("10.1038/nature12373"),
            Some(PaperId::Doi("10.1038/nature12373".into()))
        );
    }

    #[test]
    fn parse_arxiv_prefixed() {
        assert_eq!(
            PaperId::parse("arXiv:2401.11660"),
            Some(PaperId::ArXiv("2401.11660".into()))
        );
    }

    #[test]
    fn parse_arxiv_old_style() {
        assert_eq!(
            PaperId::parse("arXiv:hep-th/9905111"),
            Some(PaperId::ArXiv("hep-th/9905111".into()))
        );
    }

    #[test]
    fn parse_arxiv_doi() {
        assert_eq!(
            PaperId::parse("10.48550/arXiv.2401.11660"),
            Some(PaperId::ArXiv("2401.11660".into()))
        );
        // lowercase variant
        assert_eq!(
            PaperId::parse("10.48550/arxiv.2401.11660"),
            Some(PaperId::ArXiv("2401.11660".into()))
        );
    }

    #[test]
    fn parse_pmid() {
        assert_eq!(
            PaperId::parse("PMID:12345678"),
            Some(PaperId::Pmid("12345678".into()))
        );
    }

    #[test]
    fn parse_isbn() {
        assert_eq!(
            PaperId::parse("ISBN:978-0-123456-47-2"),
            Some(PaperId::Isbn("978-0-123456-47-2".into()))
        );
    }

    #[test]
    fn parse_empty_and_garbage() {
        assert_eq!(PaperId::parse(""), None);
        assert_eq!(PaperId::parse("  "), None);
        assert_eq!(PaperId::parse("random garbage"), None);
    }

    #[test]
    fn round_trip_stored_string() {
        let cases = [
            PaperId::Doi("10.1038/nature12373".into()),
            PaperId::ArXiv("2401.11660".into()),
            PaperId::Pmid("12345678".into()),
            PaperId::Isbn("978-0-123456-47-2".into()),
        ];
        for id in &cases {
            let stored = id.to_stored_string();
            let parsed = PaperId::parse(&stored).unwrap();
            assert_eq!(&parsed, id, "round-trip failed for {stored}");
        }
    }

    #[test]
    fn semantic_scholar_query_formats() {
        assert_eq!(
            PaperId::Doi("10.1038/nature12373".into()).semantic_scholar_query(),
            "DOI:10.1038/nature12373"
        );
        assert_eq!(
            PaperId::ArXiv("2401.11660".into()).semantic_scholar_query(),
            "ARXIV:2401.11660"
        );
        assert_eq!(
            PaperId::Pmid("12345678".into()).semantic_scholar_query(),
            "PMID:12345678"
        );
    }
}

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
        if let Some(id) = raw
            .strip_prefix("PMID:")
            .or_else(|| raw.strip_prefix("pmid:"))
        {
            let id = id.trim();
            if !id.is_empty() {
                return Some(Self::Pmid(id.to_string()));
            }
        }

        // ISBN
        if let Some(id) = raw
            .strip_prefix("ISBN:")
            .or_else(|| raw.strip_prefix("isbn:"))
        {
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

    /// Canonical web URL that resolves this identifier in a browser.
    ///
    /// Only real DOIs go to `doi.org`; arXiv's `arXiv:ID` pseudo-DOI is not a
    /// DOI and doi.org rejects it, so it resolves to its arXiv abstract page.
    pub fn resolve_url(&self) -> String {
        match self {
            Self::Doi(d) => format!("https://doi.org/{d}"),
            Self::ArXiv(id) => format!("https://arxiv.org/abs/{id}"),
            Self::Pmid(id) => format!("https://pubmed.ncbi.nlm.nih.gov/{id}/"),
            Self::Isbn(id) => format!("https://www.worldcat.org/isbn/{id}"),
        }
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
    /// ISBN, for books and book sections.
    pub isbn: Option<String>,
    /// ISSN, for serial publications.
    pub issn: Option<String>,
    /// Series title the work belongs to.
    pub series: Option<String>,
    /// Place of publication.
    pub place: Option<String>,
    /// Language of the work.
    pub language: Option<String>,
}

/// The role a [`Creator`] plays for a work. Mirrors Zotero's creator types over
/// the roles the translators emit; unknown roles round-trip through [`Other`].
///
/// [`Other`]: CreatorRole::Other
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreatorRole {
    Author,
    Editor,
    Translator,
    SeriesEditor,
    Inventor,
    Director,
    Contributor,
    /// Any role not otherwise modeled, preserving the raw Zotero creator type.
    Other(String),
}

impl CreatorRole {
    /// Parse a Zotero `creatorType` string. An empty type is treated as author,
    /// matching how the translators leave the primary creator's type implicit.
    pub fn from_zotero(creator_type: &str) -> Self {
        match creator_type {
            "" | "author" => Self::Author,
            "editor" => Self::Editor,
            "translator" => Self::Translator,
            "seriesEditor" => Self::SeriesEditor,
            "inventor" => Self::Inventor,
            "director" => Self::Director,
            "contributor" => Self::Contributor,
            other => Self::Other(other.to_string()),
        }
    }

    /// The Zotero `creatorType` string for this role.
    pub fn as_zotero(&self) -> &str {
        match self {
            Self::Author => "author",
            Self::Editor => "editor",
            Self::Translator => "translator",
            Self::SeriesEditor => "seriesEditor",
            Self::Inventor => "inventor",
            Self::Director => "director",
            Self::Contributor => "contributor",
            Self::Other(s) => s,
        }
    }

    /// Whether this creator is an author (the role the flat display list shows).
    pub fn is_author(&self) -> bool {
        matches!(self, Self::Author)
    }
}

/// An author or contributor on a paper: a structured name plus a role.
///
/// A personal name splits into `first_name`/`last_name`; an institutional name
/// (a corporation, committee, …) uses the single `name` field instead, leaving
/// the split fields empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Creator {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub first_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub last_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub role: CreatorRole,
}

impl Creator {
    /// A personal author from a first and last name.
    pub fn author(first_name: impl Into<String>, last_name: impl Into<String>) -> Self {
        Self {
            first_name: first_name.into(),
            last_name: last_name.into(),
            name: String::new(),
            role: CreatorRole::Author,
        }
    }

    /// An author parsed from a "First Last" display string: the text before the
    /// last space is the given name, the remainder the surname. A single-token
    /// name is treated as an institutional/mononym `name`. Used by the
    /// bibliography importers, which parse author names into display strings.
    pub fn author_from_display(display: &str) -> Self {
        match display.trim().rsplit_once(' ') {
            Some((first, last)) => Self::author(first.trim(), last.trim()),
            None => Self {
                first_name: String::new(),
                last_name: String::new(),
                name: display.trim().to_string(),
                role: CreatorRole::Author,
            },
        }
    }

    /// The creator rendered as a display name: the institutional `name` if set,
    /// else "First Last" (or whichever half is present).
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if self.first_name.is_empty() {
            self.last_name.clone()
        } else if self.last_name.is_empty() {
            self.first_name.clone()
        } else {
            format!("{} {}", self.first_name, self.last_name)
        }
    }
}

impl<'de> Deserialize<'de> for Creator {
    /// Accept both the current object shape (`{firstName, lastName, name, role}`)
    /// and the legacy shape (a bare display-name string, `"John Doe"`) that older
    /// DB rows stored for the `authors` column. A legacy string is split on the
    /// last space into first/last and assigned the author role.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CreatorObj {
            #[serde(default)]
            first_name: String,
            #[serde(default)]
            last_name: String,
            #[serde(default)]
            name: String,
            #[serde(default = "default_role")]
            role: CreatorRole,
        }
        fn default_role() -> CreatorRole {
            CreatorRole::Author
        }

        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Str(String),
            Obj(CreatorObj),
        }

        Ok(match Repr::deserialize(deserializer)? {
            Repr::Str(display) => match display.rsplit_once(' ') {
                Some((first, last)) => Creator::author(first.trim(), last.trim()),
                None => Creator {
                    first_name: String::new(),
                    last_name: String::new(),
                    name: display,
                    role: CreatorRole::Author,
                },
            },
            Repr::Obj(o) => Creator {
                first_name: o.first_name,
                last_name: o.last_name,
                name: o.name,
                role: o.role,
            },
        })
    }
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

/// Which online source a web-search result came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    OpenAlex,
    ArXiv,
    SemanticScholar,
}

/// Transient relevance signal attached to a web-search result. Not persisted —
/// it exists only on the in-memory results of a live search so they can be
/// ranked, and is `None` on any paper loaded from the DB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchRank {
    /// The source this result came from.
    pub source: ProviderKind,
    /// The API's raw relevance score, if the provider returns one (OpenAlex,
    /// Semantic Scholar). `None` for arXiv, which has no score.
    pub raw_score: Option<f64>,
    /// Zero-based position in that provider's returned batch (used as the
    /// relevance signal when `raw_score` is absent).
    pub position: usize,
}

/// The Zotero item type when a paper carries no explicit one.
fn default_item_type() -> String {
    "journalArticle".to_string()
}

/// A research paper with full metadata, links, library status, and citation info.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paper {
    pub id: Option<String>,
    /// The Zotero item type (`journalArticle`, `book`, `conferencePaper`, …).
    /// Drives per-type citation formatting and venue labeling.
    #[serde(default = "default_item_type")]
    pub item_type: String,
    pub title: String,
    pub creators: Vec<Creator>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub abstract_text: Option<String>,
    pub publication: Publication,
    pub links: PaperLinks,
    pub status: LibraryStatus,
    pub citation: CitationInfo,
    /// Live web-search relevance signal; never serialized (see [`SearchRank`]).
    #[serde(skip)]
    pub search_rank: Option<SearchRank>,
}

impl Default for Paper {
    fn default() -> Self {
        Self {
            id: None,
            item_type: default_item_type(),
            title: String::new(),
            creators: Vec::new(),
            year: None,
            doi: None,
            abstract_text: None,
            publication: Publication::default(),
            links: PaperLinks::default(),
            status: LibraryStatus::default(),
            citation: CitationInfo::default(),
            search_rank: None,
        }
    }
}

impl Paper {
    /// Create a new paper with the given title and default values for all other fields.
    pub fn new(title: String) -> Self {
        Self {
            title,
            ..Default::default()
        }
    }

    /// The display names of the paper's authors, in order. Non-author creators
    /// (editors, translators, …) are excluded; this is the drop-in for call sites
    /// that treated authors as a flat list of display strings.
    pub fn author_names(&self) -> Vec<String> {
        self.creators
            .iter()
            .filter(|c| c.role.is_author())
            .map(|c| c.display_name())
            .collect()
    }

    /// Format authors for display: "Unknown", "A, B", or "A et al."
    pub fn formatted_authors(&self) -> String {
        let names = self.author_names();
        if names.is_empty() {
            "Unknown".to_string()
        } else if names.len() <= 2 {
            names.join(", ")
        } else {
            format!("{} et al.", names[0])
        }
    }

    /// Parse the `doi` field into a typed [`PaperId`], if present and recognized.
    pub fn paper_id(&self) -> Option<PaperId> {
        self.doi.as_deref().and_then(PaperId::parse)
    }

    /// Web URL that resolves this paper's identifier in a browser, or `None` if
    /// it has no identifier. Routes arXiv/PMID/ISBN to their own resolvers
    /// rather than doi.org (which rejects non-DOIs like `arXiv:ID`); an
    /// unrecognized but non-empty `doi` falls back to doi.org.
    pub fn resolve_url(&self) -> Option<String> {
        if let Some(pid) = self.paper_id() {
            return Some(pid.resolve_url());
        }
        match self.doi.as_deref() {
            Some(d) if !d.is_empty() => Some(format!("https://doi.org/{d}")),
            _ => None,
        }
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
        if !self.creators.is_empty() {
            c += 1;
        }
        c
    }
}

/// Normalize a title for fuzzy comparison: lowercase, strip punctuation, and
/// collapse whitespace. Used for duplicate detection and exact-match ranking.
pub fn normalize_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Build the match string passed to turso's `fts_match()` from a raw user query.
///
/// turso combines bare space-separated tokens with **OR**, and does not strip
/// stopwords — so `a bitter lesson` matches any paper containing "a", which is
/// nearly all of them, and BM25 then surfaces long documents that merely repeat
/// common words rather than papers about the actual topic. Joining the tokens
/// with `AND` requires every term to be present, which is the behavior users
/// expect from a search box (and correctly returns nothing when the searched
/// title isn't in the library, rather than a confident wrong answer).
///
/// Each token is quoted so punctuation (hyphens, colons) and the FTS operator
/// keywords (`AND`/`OR`/`NOT`) a user might type are treated as literal terms,
/// not query syntax. Returns an empty string when the query has no usable
/// tokens, which callers should treat as "no FTS query".
pub fn build_fts_match_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|tok| {
            // Keep alphanumerics; drop other punctuation so a token like
            // "low-rank" becomes the phrase "low rank" and colons/quotes can't
            // break out of the quoted term.
            let cleaned: String = tok
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { ' ' })
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            cleaned
        })
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// Relevance score of a local library paper against a query. Higher is better.
///
/// Exact and prefix normalized-title matches dominate so the searched-for paper
/// ranks first; below that, results order by how many query tokens appear in the
/// title (weighted heavily) and abstract (lightly), with citation count and
/// recency as mild tiebreakers.
pub fn local_relevance_score(paper: &Paper, query_norm: &str, query_tokens: &[&str]) -> f64 {
    if query_norm.is_empty() {
        return 0.0;
    }
    let nt = normalize_title(&paper.title);
    let mut s = 0.0;

    if nt == query_norm {
        s += 1000.0;
    } else if nt.starts_with(query_norm) {
        s += 400.0;
    } else if nt.contains(query_norm) {
        s += 250.0;
    }

    if !query_tokens.is_empty() {
        let title_tokens: Vec<&str> = nt.split_whitespace().collect();
        let title_hits = query_tokens
            .iter()
            .filter(|t| title_tokens.contains(t))
            .count();
        s += 120.0 * (title_hits as f64) / (query_tokens.len() as f64);

        if let Some(abstract_text) = &paper.abstract_text {
            let na = normalize_title(abstract_text);
            let abs_hits = query_tokens.iter().filter(|t| na.contains(**t)).count();
            s += 20.0 * (abs_hits as f64) / (query_tokens.len() as f64);
        }
    }

    let citations = paper.citation.citation_count.unwrap_or(0).max(0) as f64;
    s += 2.0 * (1.0 + citations).ln();

    if let Some(year) = paper.year {
        s += 0.1 * ((year - 2000).clamp(0, 25) as f64);
    }

    s
}

/// Sort local search candidates by [`local_relevance_score`], best first.
pub fn rank_local_results(mut papers: Vec<Paper>, query: &str) -> Vec<Paper> {
    let nq = normalize_title(query);
    if nq.is_empty() {
        return papers;
    }
    let tokens: Vec<&str> = nq.split_whitespace().collect();
    papers.sort_by(|a, b| {
        let sa = local_relevance_score(a, &nq, &tokens);
        let sb = local_relevance_score(b, &nq, &tokens);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.title.cmp(&b.title))
    });
    papers.truncate(50);
    papers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titled(title: &str) -> Paper {
        Paper {
            title: title.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn rank_puts_exact_title_first() {
        // Exact title given last, buried among partial matches.
        let papers = vec![
            titled("Attention and Working Memory"),
            titled("You Only Need Attention Sometimes"),
            titled("A Study of Needs"),
            titled("Attention Is All You Need"),
        ];
        let ranked = rank_local_results(papers, "attention is all you need");
        assert_eq!(ranked[0].title, "Attention Is All You Need");
    }

    #[test]
    fn rank_prefers_more_token_overlap() {
        let papers = vec![
            titled("Deep Learning"),
            titled("Neural Machine Translation by Jointly Learning to Align"),
        ];
        let ranked = rank_local_results(papers, "neural machine translation");
        assert!(ranked[0].title.starts_with("Neural Machine Translation"));
    }

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
    fn creator_legacy_string_deserializes() {
        // Old DB rows stored the authors column as an array of display strings.
        let creators: Vec<Creator> = serde_json::from_str(r#"["John Doe", "Jane"]"#).unwrap();
        assert_eq!(creators.len(), 2);
        assert_eq!(creators[0], Creator::author("John", "Doe"));
        assert_eq!(creators[0].role, CreatorRole::Author);
        // A single-token name has no first/last split.
        assert_eq!(creators[1].name, "Jane");
        assert_eq!(creators[1].display_name(), "Jane");
    }

    #[test]
    fn creator_object_deserializes_with_role() {
        let creators: Vec<Creator> = serde_json::from_str(
            r#"[{"firstName":"A","lastName":"Ed","role":"Editor"},
                {"name":"World Health Organization","role":"Author"}]"#,
        )
        .unwrap();
        assert_eq!(creators[0].role, CreatorRole::Editor);
        assert_eq!(creators[0].display_name(), "A Ed");
        assert_eq!(creators[1].display_name(), "World Health Organization");
    }

    #[test]
    fn creator_roundtrips_through_json() {
        let creators = vec![
            Creator::author("Ada", "Lovelace"),
            Creator {
                first_name: "Grace".into(),
                last_name: "Hopper".into(),
                name: String::new(),
                role: CreatorRole::Editor,
            },
        ];
        let json = serde_json::to_string(&creators).unwrap();
        let back: Vec<Creator> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, creators);
    }

    #[test]
    fn author_names_excludes_non_authors() {
        let paper = Paper {
            creators: vec![
                Creator::author("Ada", "Lovelace"),
                Creator {
                    first_name: "Ed".into(),
                    last_name: "Itor".into(),
                    name: String::new(),
                    role: CreatorRole::Editor,
                },
            ],
            ..Default::default()
        };
        assert_eq!(paper.author_names(), vec!["Ada Lovelace"]);
        assert_eq!(paper.formatted_authors(), "Ada Lovelace");
    }

    #[test]
    fn paper_defaults_to_journal_article() {
        assert_eq!(Paper::default().item_type, "journalArticle");
        // A full paper JSON that omits item_type (pre-enrichment shape) still reads,
        // falling back to the journalArticle default.
        let mut value = serde_json::to_value(Paper::default()).unwrap();
        value.as_object_mut().unwrap().remove("item_type");
        let p: Paper = serde_json::from_value(value).unwrap();
        assert_eq!(p.item_type, "journalArticle");
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

    #[test]
    fn resolve_url_routes_by_identifier_type() {
        assert_eq!(
            PaperId::Doi("10.1038/nature12373".into()).resolve_url(),
            "https://doi.org/10.1038/nature12373"
        );
        // arXiv must NOT go to doi.org (it rejects the pseudo-DOI).
        assert_eq!(
            PaperId::ArXiv("2508.18081".into()).resolve_url(),
            "https://arxiv.org/abs/2508.18081"
        );
        assert_eq!(
            PaperId::Pmid("12345678".into()).resolve_url(),
            "https://pubmed.ncbi.nlm.nih.gov/12345678/"
        );
    }

    #[test]
    fn paper_resolve_url_handles_arxiv_stored_doi() {
        // A Paper whose stored `doi` is the arXiv pseudo-DOI resolves to arXiv.
        let mut p = titled("Some Preprint");
        p.doi = Some("arXiv:2508.18081".into());
        assert_eq!(
            p.resolve_url().as_deref(),
            Some("https://arxiv.org/abs/2508.18081")
        );

        // No identifier -> no URL.
        let bare = titled("No DOI");
        assert_eq!(bare.resolve_url(), None);
    }

    #[test]
    fn fts_query_and_joins_tokens() {
        // Every token becomes a required, quoted term — so a paper must contain
        // all of them, not just one common word.
        assert_eq!(
            build_fts_match_query("a bitter lesson"),
            "\"a\" AND \"bitter\" AND \"lesson\""
        );
    }

    #[test]
    fn fts_query_splits_punctuation_into_phrase() {
        // Hyphens/colons become spaces inside the quoted term rather than FTS
        // syntax, so "low-rank" stays a single phrase term.
        assert_eq!(
            build_fts_match_query("low-rank adaptation"),
            "\"low rank\" AND \"adaptation\""
        );
    }

    #[test]
    fn fts_query_empty_when_no_tokens() {
        assert_eq!(build_fts_match_query("   "), "");
        assert_eq!(build_fts_match_query("!!! ---"), "");
    }
}

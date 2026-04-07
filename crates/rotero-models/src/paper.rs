use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

impl Default for Paper {
    fn default() -> Self {
        Self {
            id: None,
            title: String::new(),
            authors: Vec::new(),
            year: None,
            doi: None,
            abstract_text: None,
            publication: Publication::default(),
            links: PaperLinks::default(),
            status: LibraryStatus::default(),
            citation: CitationInfo::default(),
        }
    }
}

impl Paper {
    pub fn new(title: String) -> Self {
        Self {
            title,
            ..Default::default()
        }
    }
}

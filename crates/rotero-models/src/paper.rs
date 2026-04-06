use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Paper {
    pub id: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub abstract_text: Option<String>,
    pub journal: Option<String>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub url: Option<String>,
    pub pdf_url: Option<String>,
    pub pdf_path: Option<String>,
    pub date_added: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
    pub is_favorite: bool,
    pub is_read: bool,
    pub citation_count: Option<i64>,
    pub citation_key: Option<String>,
    pub extra_meta: Option<serde_json::Value>,
}

impl Paper {
    pub fn new(title: String) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            authors: Vec::new(),
            year: None,
            doi: None,
            abstract_text: None,
            journal: None,
            volume: None,
            issue: None,
            pages: None,
            publisher: None,
            url: None,
            pdf_url: None,
            pdf_path: None,
            date_added: now,
            date_modified: now,
            is_favorite: false,
            is_read: false,
            citation_count: None,
            citation_key: None,
            extra_meta: None,
        }
    }
}

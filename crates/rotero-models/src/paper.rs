use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paper {
    pub id: Option<i64>,
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
    pub pdf_path: Option<String>,
    pub date_added: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
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
            pdf_path: None,
            date_added: now,
            date_modified: now,
            extra_meta: None,
        }
    }
}

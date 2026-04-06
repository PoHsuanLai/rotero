use serde::{Deserialize, Serialize};

/// A Zotero item as returned by the translation server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroItem {
    #[serde(default)]
    pub item_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub creators: Vec<ZoteroCreator>,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "DOI", default)]
    pub doi: String,
    #[serde(rename = "ISBN", default)]
    pub isbn: String,
    #[serde(rename = "ISSN", default)]
    pub issn: String,
    #[serde(default)]
    pub abstract_note: String,
    #[serde(default)]
    pub publication_title: String,
    #[serde(default)]
    pub volume: String,
    #[serde(default)]
    pub issue: String,
    #[serde(default)]
    pub pages: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub place: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub attachments: Vec<ZoteroAttachment>,
    #[serde(default)]
    pub tags: Vec<ZoteroTag>,
    #[serde(default)]
    pub extra: String,
    #[serde(default)]
    pub access_date: String,
    #[serde(default)]
    pub journal_abbreviation: String,
    #[serde(default)]
    pub short_title: String,
    #[serde(default)]
    pub series: String,

    /// Catch-all for fields we don't explicitly model.
    #[serde(flatten)]
    pub extra_fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroCreator {
    #[serde(default)]
    pub first_name: String,
    #[serde(default)]
    pub last_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub creator_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroAttachment {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub snapshot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroTag {
    pub tag: String,
    #[serde(rename = "type", default)]
    pub tag_type: i32,
}

impl ZoteroItem {
    /// Get the PDF download URL from attachments (populated by patched translation-server).
    pub fn pdf_url(&self) -> Option<String> {
        // First: check attachments from the translator
        for att in &self.attachments {
            if att.mime_type == "application/pdf" && !att.url.is_empty() {
                return Some(att.url.clone());
            }
        }
        None
    }

    /// Convert to a rotero Paper model.
    pub fn into_paper(self) -> Option<rotero_models::Paper> {
        if self.title.is_empty() || self.item_type == "note" || self.item_type == "attachment" {
            return None;
        }

        let mut paper = rotero_models::Paper::new(self.title);

        if !self.doi.is_empty() {
            paper.doi = Some(self.doi);
        }
        if !self.url.is_empty() {
            paper.url = Some(self.url);
        }
        if !self.publication_title.is_empty() {
            paper.journal = Some(self.publication_title);
        }
        if !self.volume.is_empty() {
            paper.volume = Some(self.volume);
        }
        if !self.issue.is_empty() {
            paper.issue = Some(self.issue);
        }
        if !self.pages.is_empty() {
            paper.pages = Some(self.pages);
        }
        if !self.publisher.is_empty() {
            paper.publisher = Some(self.publisher);
        }
        if !self.abstract_note.is_empty() {
            paper.abstract_text = Some(self.abstract_note);
        }

        // Parse year from date
        if !self.date.is_empty() {
            paper.year = extract_year(&self.date);
        }

        // Build author list
        paper.authors = self
            .creators
            .into_iter()
            .filter(|c| c.creator_type.is_empty() || c.creator_type == "author")
            .map(|c| {
                if !c.name.is_empty() {
                    c.name
                } else if c.first_name.is_empty() {
                    c.last_name
                } else {
                    format!("{} {}", c.first_name, c.last_name)
                }
            })
            .filter(|s| !s.is_empty())
            .collect();

        Some(paper)
    }
}

fn extract_year(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit() {
            if let Ok(year) = s[i..i + 4].parse::<i32>() {
                if (1900..=2100).contains(&year) {
                    return Some(year);
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_year() {
        assert_eq!(extract_year("2024-01-15"), Some(2024));
        assert_eq!(extract_year("January 2023"), Some(2023));
        assert_eq!(extract_year("no year"), None);
    }

    #[test]
    fn test_zotero_item_to_paper() {
        let item = ZoteroItem {
            item_type: "journalArticle".into(),
            title: "Test Paper".into(),
            creators: vec![ZoteroCreator {
                first_name: "John".into(),
                last_name: "Doe".into(),
                name: String::new(),
                creator_type: "author".into(),
            }],
            date: "2024".into(),
            doi: "10.1234/test".into(),
            ..Default::default()
        };
        let paper = item.into_paper().unwrap();
        assert_eq!(paper.title, "Test Paper");
        assert_eq!(paper.doi, Some("10.1234/test".into()));
        assert_eq!(paper.year, Some(2024));
        assert_eq!(paper.authors, vec!["John Doe"]);
    }
}

impl Default for ZoteroItem {
    fn default() -> Self {
        Self {
            item_type: String::new(),
            title: String::new(),
            creators: Vec::new(),
            date: String::new(),
            url: String::new(),
            doi: String::new(),
            isbn: String::new(),
            issn: String::new(),
            abstract_note: String::new(),
            publication_title: String::new(),
            volume: String::new(),
            issue: String::new(),
            pages: String::new(),
            publisher: String::new(),
            place: String::new(),
            language: String::new(),
            attachments: Vec::new(),
            tags: Vec::new(),
            extra: String::new(),
            access_date: String::new(),
            journal_abbreviation: String::new(),
            short_title: String::new(),
            series: String::new(),
            extra_fields: serde_json::Map::new(),
        }
    }
}

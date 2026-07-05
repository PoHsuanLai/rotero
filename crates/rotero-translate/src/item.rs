use rotero_models::PaperId;
use serde::{Deserialize, Serialize};

/// A Zotero item as returned by the translation server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

/// An author or contributor associated with a Zotero item.
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

/// A file attachment (typically a PDF) linked to a Zotero item.
///
/// Either `url` (a remote file, from web/API translators) or `path` (a local
/// file, from bibliography imports) is set — the other is empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ZoteroAttachment {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    /// Local file path (linked/imported file). Empty for remote attachments.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub snapshot: bool,
}

/// A keyword tag attached to a Zotero item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoteroTag {
    pub tag: String,
    #[serde(rename = "type", default)]
    pub tag_type: i32,
}

impl ZoteroItem {
    /// Get the PDF download URL from attachments (populated by patched translation-server).
    pub fn pdf_url(&self) -> Option<String> {
        for att in &self.attachments {
            if att.mime_type == "application/pdf" && !att.url.is_empty() {
                return Some(att.url.clone());
            }
        }
        None
    }

    /// Get a local PDF file path from attachments, if one was set (imports).
    pub fn pdf_path(&self) -> Option<String> {
        for att in &self.attachments {
            if !att.path.is_empty() {
                return Some(att.path.clone());
            }
        }
        None
    }

    /// Build a `ZoteroItem` from a flat [`Paper`](rotero_models::Paper), used by
    /// the DOI-content-negotiation and bibliography-import translators. Authors
    /// are split on the last space into first and last names.
    pub fn from_paper(p: rotero_models::Paper) -> Self {
        let creators = p
            .authors
            .into_iter()
            .map(|name| match name.rsplit_once(' ') {
                Some((first, last)) => ZoteroCreator {
                    first_name: first.trim().to_string(),
                    last_name: last.trim().to_string(),
                    name: String::new(),
                    creator_type: "author".to_string(),
                },
                None => ZoteroCreator {
                    first_name: String::new(),
                    last_name: String::new(),
                    name,
                    creator_type: "author".to_string(),
                },
            })
            .collect();

        // Preserve any PDF the Paper already carried, as an attachment.
        let mut attachments = Vec::new();
        if let Some(url) = p.links.pdf_url.filter(|s| !s.is_empty()) {
            attachments.push(ZoteroAttachment {
                title: "Full Text PDF".to_string(),
                url,
                mime_type: "application/pdf".to_string(),
                ..Default::default()
            });
        }
        if let Some(path) = p.links.pdf_path.filter(|s| !s.is_empty()) {
            attachments.push(ZoteroAttachment {
                title: "Full Text PDF".to_string(),
                path,
                mime_type: "application/pdf".to_string(),
                ..Default::default()
            });
        }

        ZoteroItem {
            item_type: "journalArticle".to_string(),
            title: p.title,
            creators,
            date: p.year.map(|y| y.to_string()).unwrap_or_default(),
            doi: p.doi.unwrap_or_default(),
            abstract_note: p.abstract_text.unwrap_or_default(),
            publication_title: p.publication.journal.unwrap_or_default(),
            volume: p.publication.volume.unwrap_or_default(),
            issue: p.publication.issue.unwrap_or_default(),
            pages: p.publication.pages.unwrap_or_default(),
            publisher: p.publication.publisher.unwrap_or_default(),
            url: p.links.url.unwrap_or_default(),
            attachments,
            ..Default::default()
        }
    }

    /// Convert this Zotero item into a [`Paper`](rotero_models::Paper), returning
    /// `None` for notes, attachments, or items with empty titles.
    pub fn into_paper(self) -> Option<rotero_models::Paper> {
        if self.title.is_empty() || self.item_type == "note" || self.item_type == "attachment" {
            return None;
        }

        let non_empty = |s: String| -> Option<String> { if s.is_empty() { None } else { Some(s) } };

        // Carry a remote PDF URL through. A local attachment `path` is left out
        // here: it's typically a relative path the caller must resolve/import
        // against a base dir before it can be stored (see the import UI).
        let pdf_url = self.pdf_url();

        let authors: Vec<String> = self
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

        Some(rotero_models::Paper {
            title: self.title,
            authors,
            year: if self.date.is_empty() {
                None
            } else {
                extract_year(&self.date)
            },
            doi: non_empty(self.doi)
                .or_else(|| non_empty(self.isbn).map(|i| PaperId::Isbn(i).to_stored_string()))
                .or_else(|| extract_pmid(&self.extra)),
            abstract_text: non_empty(self.abstract_note),
            publication: rotero_models::Publication {
                journal: non_empty(self.publication_title),
                volume: non_empty(self.volume),
                issue: non_empty(self.issue),
                pages: non_empty(self.pages),
                publisher: non_empty(self.publisher),
            },
            links: rotero_models::PaperLinks {
                url: non_empty(self.url),
                pdf_url,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

/// Extract a PMID from the Zotero `extra` field (e.g. "PMID: 12345678").
fn extract_pmid(extra: &str) -> Option<String> {
    for line in extra.lines() {
        let line = line.trim();
        if let Some(rest) = line
            .strip_prefix("PMID:")
            .or_else(|| line.strip_prefix("pmid:"))
        {
            let id = rest.trim();
            if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
                return Some(PaperId::Pmid(id.to_string()).to_stored_string());
            }
        }
    }
    None
}

fn extract_year(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit()
            && let Ok(year) = s[i..i + 4].parse::<i32>()
            && (1900..=2100).contains(&year)
        {
            return Some(year);
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

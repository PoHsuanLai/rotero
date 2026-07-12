use rotero_models::PaperId;
use serde::{Deserialize, Serialize};

/// A Zotero item — the uniform currency the translators produce (a serde mirror
/// of the Zotero item JSON shape).
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
///
/// Upstream translators emit tags either as a bare string (`"keyword"`) or as
/// an object (`{ "tag": "keyword", "type": 1 }`); the custom [`Deserialize`]
/// below accepts both. Serialization always uses the object form.
#[derive(Debug, Clone, Serialize)]
pub struct ZoteroTag {
    pub tag: String,
    #[serde(rename = "type", default)]
    pub tag_type: i32,
}

impl<'de> Deserialize<'de> for ZoteroTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // A tag is either a plain string or a { tag, type } object.
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Str(String),
            Obj {
                #[serde(default)]
                tag: String,
                #[serde(rename = "type", default)]
                tag_type: i32,
            },
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Str(tag) => ZoteroTag { tag, tag_type: 0 },
            Repr::Obj { tag, tag_type } => ZoteroTag { tag, tag_type },
        })
    }
}

impl ZoteroItem {
    /// The first PDF attachment's download URL, if any. Engine-produced items
    /// have relative URLs resolved against the page URL before this is read.
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

    /// Build a `ZoteroItem` from a [`Paper`](rotero_models::Paper), used by the
    /// DOI-content-negotiation and bibliography-import translators. Structured
    /// creators and roles carry over directly.
    pub fn from_paper(p: rotero_models::Paper) -> Self {
        let creators = p
            .creators
            .into_iter()
            .map(|c| ZoteroCreator {
                first_name: c.first_name,
                last_name: c.last_name,
                name: c.name,
                creator_type: c.role.as_zotero().to_string(),
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
            item_type: p.item_type,
            title: p.title,
            creators,
            date: p.year.map(|y| y.to_string()).unwrap_or_default(),
            doi: p.doi.unwrap_or_default(),
            isbn: p.publication.isbn.unwrap_or_default(),
            issn: p.publication.issn.unwrap_or_default(),
            abstract_note: p.abstract_text.unwrap_or_default(),
            publication_title: p.publication.journal.unwrap_or_default(),
            volume: p.publication.volume.unwrap_or_default(),
            issue: p.publication.issue.unwrap_or_default(),
            pages: p.publication.pages.unwrap_or_default(),
            publisher: p.publication.publisher.unwrap_or_default(),
            series: p.publication.series.unwrap_or_default(),
            place: p.publication.place.unwrap_or_default(),
            language: p.publication.language.unwrap_or_default(),
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

        // Carry every creator with its role, keeping structured names. Creators
        // with no usable name at all are dropped.
        let creators: Vec<rotero_models::Creator> = self
            .creators
            .into_iter()
            .map(|c| rotero_models::Creator {
                first_name: c.first_name,
                last_name: c.last_name,
                name: c.name,
                role: rotero_models::CreatorRole::from_zotero(&c.creator_type),
            })
            .filter(|c| !c.display_name().is_empty())
            .collect();

        let isbn = non_empty(self.isbn);

        Some(rotero_models::Paper {
            item_type: if self.item_type.is_empty() {
                "journalArticle".to_string()
            } else {
                self.item_type
            },
            title: self.title,
            creators,
            year: if self.date.is_empty() {
                None
            } else {
                extract_year(&self.date)
            },
            // DOI proper only; ISBN has its own venue field and no longer
            // masquerades as a DOI. PMID from the extra field is a last resort.
            doi: non_empty(self.doi).or_else(|| extract_pmid(&self.extra)),
            abstract_text: non_empty(self.abstract_note),
            publication: rotero_models::Publication {
                journal: non_empty(self.publication_title),
                volume: non_empty(self.volume),
                issue: non_empty(self.issue),
                pages: non_empty(self.pages),
                publisher: non_empty(self.publisher),
                isbn,
                issn: non_empty(self.issn),
                series: non_empty(self.series),
                place: non_empty(self.place),
                language: non_empty(self.language),
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
            item_type: "book".into(),
            title: "Test Paper".into(),
            creators: vec![
                ZoteroCreator {
                    first_name: "John".into(),
                    last_name: "Doe".into(),
                    name: String::new(),
                    creator_type: "author".into(),
                },
                ZoteroCreator {
                    first_name: "Ed".into(),
                    last_name: "Itor".into(),
                    name: String::new(),
                    creator_type: "editor".into(),
                },
            ],
            date: "2024".into(),
            doi: "10.1234/test".into(),
            isbn: "978-0-13-468599-1".into(),
            series: "Great Works".into(),
            ..Default::default()
        };
        let paper = item.into_paper().unwrap();
        assert_eq!(paper.title, "Test Paper");
        // item_type carries through instead of collapsing to journalArticle.
        assert_eq!(paper.item_type, "book");
        assert_eq!(paper.doi, Some("10.1234/test".into()));
        assert_eq!(paper.year, Some(2024));
        // Both creators survive with their roles; author_names shows only authors.
        assert_eq!(paper.creators.len(), 2);
        assert_eq!(paper.creators[1].role, rotero_models::CreatorRole::Editor);
        assert_eq!(paper.author_names(), vec!["John Doe"]);
        // Venue fields land in their own slots, not the DOI.
        assert_eq!(paper.publication.isbn.as_deref(), Some("978-0-13-468599-1"));
        assert_eq!(paper.publication.series.as_deref(), Some("Great Works"));
    }
}

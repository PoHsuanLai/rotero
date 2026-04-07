use rotero_models::{Paper, PaperLinks, Publication};
use serde::Deserialize;

/// CSL-JSON is the standard JSON format used by Zotero, Mendeley, and others.
pub fn import_csl_json(input: &str) -> Result<Vec<Paper>, String> {
    let items: Vec<CslItem> =
        serde_json::from_str(input).map_err(|e| format!("Failed to parse CSL-JSON: {e}"))?;

    let papers: Vec<Paper> = items
        .into_iter()
        .filter_map(|item| item.into_paper())
        .collect();

    if papers.is_empty() {
        return Err("No valid items found in CSL-JSON file".to_string());
    }

    Ok(papers)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CslItem {
    title: Option<String>,
    #[serde(default)]
    author: Vec<CslName>,
    #[serde(rename = "DOI")]
    doi: Option<String>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    container_title: Option<String>,
    volume: Option<StringOrNumber>,
    issue: Option<StringOrNumber>,
    page: Option<String>,
    publisher: Option<String>,
    #[serde(rename = "URL")]
    url: Option<String>,
    issued: Option<CslDate>,
}

#[derive(Debug, Deserialize)]
struct CslName {
    family: Option<String>,
    given: Option<String>,
    literal: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct CslDate {
    #[serde(default)]
    date_parts: Vec<Vec<serde_json::Number>>,
    raw: Option<String>,
}

/// CSL-JSON sometimes uses strings, sometimes numbers for volume/issue.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    Str(String),
    Num(serde_json::Number),
}

impl std::fmt::Display for StringOrNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StringOrNumber::Str(s) => write!(f, "{s}"),
            StringOrNumber::Num(n) => write!(f, "{n}"),
        }
    }
}

impl CslItem {
    fn into_paper(self) -> Option<Paper> {
        let title = self.title?;
        if title.is_empty() {
            return None;
        }

        let authors: Vec<String> = self
            .author
            .into_iter()
            .filter_map(|name| {
                if let Some(lit) = name.literal {
                    Some(lit)
                } else {
                    match (name.given, name.family) {
                        (Some(g), Some(f)) => Some(format!("{g} {f}")),
                        (None, Some(f)) => Some(f),
                        (Some(g), None) => Some(g),
                        (None, None) => None,
                    }
                }
            })
            .collect();

        let year = self.issued.and_then(|d| {
            // Try date-parts first: [[year, month, day]]
            if let Some(parts) = d.date_parts.first()
                && let Some(y) = parts.first()
            {
                return y.as_i64().map(|v| v as i32);
            }
            // Fallback: parse raw date string
            d.raw
                .and_then(|s| s.split('-').next().and_then(|y| y.trim().parse().ok()))
        });

        Some(Paper {
            title,
            authors,
            year,
            doi: self.doi,
            abstract_text: self.abstract_text,
            publication: Publication {
                journal: self.container_title,
                volume: self.volume.map(|v| v.to_string()),
                issue: self.issue.map(|v| v.to_string()),
                pages: self.page,
                publisher: self.publisher,
            },
            links: PaperLinks {
                url: self.url,
                ..Default::default()
            },
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_csl_import() {
        let input = r#"[
            {
                "type": "article-journal",
                "title": "A test paper",
                "author": [
                    {"given": "John", "family": "Smith"},
                    {"given": "Jane", "family": "Doe"}
                ],
                "issued": {"date-parts": [[2023, 6, 15]]},
                "DOI": "10.1234/test",
                "container-title": "Nature",
                "volume": "42",
                "issue": "3",
                "page": "100-110",
                "abstract": "This is the abstract.",
                "publisher": "Springer",
                "URL": "https://example.com/paper"
            }
        ]"#;

        let papers = import_csl_json(input).unwrap();
        assert_eq!(papers.len(), 1);
        let p = &papers[0];
        assert_eq!(p.title, "A test paper");
        assert_eq!(p.authors, vec!["John Smith", "Jane Doe"]);
        assert_eq!(p.year, Some(2023));
        assert_eq!(p.doi.as_deref(), Some("10.1234/test"));
        assert_eq!(p.publication.journal.as_deref(), Some("Nature"));
        assert_eq!(p.publication.volume.as_deref(), Some("42"));
        assert_eq!(p.publication.issue.as_deref(), Some("3"));
        assert_eq!(p.publication.pages.as_deref(), Some("100-110"));
        assert_eq!(p.abstract_text.as_deref(), Some("This is the abstract."));
        assert_eq!(p.publication.publisher.as_deref(), Some("Springer"));
        assert_eq!(p.links.url.as_deref(), Some("https://example.com/paper"));
    }

    #[test]
    fn test_literal_author() {
        let input = r#"[{"title": "Test", "author": [{"literal": "WHO"}]}]"#;
        let papers = import_csl_json(input).unwrap();
        assert_eq!(papers[0].authors, vec!["WHO"]);
    }

    #[test]
    fn test_numeric_volume() {
        let input = r#"[{"title": "Test", "volume": 5, "issue": 12}]"#;
        let papers = import_csl_json(input).unwrap();
        assert_eq!(papers[0].publication.volume.as_deref(), Some("5"));
        assert_eq!(papers[0].publication.issue.as_deref(), Some("12"));
    }
}

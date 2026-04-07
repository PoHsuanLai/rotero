use rotero_models::{Paper, PaperLinks, Publication};
use serde::Deserialize;

const CROSSREF_API: &str = "https://api.crossref.org/works";

#[derive(Debug, Deserialize)]
struct CrossRefResponse {
    message: CrossRefWork,
}

#[derive(Debug, Deserialize)]
struct CrossRefWork {
    title: Option<Vec<String>>,
    author: Option<Vec<CrossRefAuthor>>,
    #[serde(rename = "container-title")]
    container_title: Option<Vec<String>>,
    volume: Option<String>,
    issue: Option<String>,
    page: Option<String>,
    publisher: Option<String>,
    #[serde(rename = "published-print")]
    published_print: Option<CrossRefDate>,
    #[serde(rename = "published-online")]
    published_online: Option<CrossRefDate>,
    #[serde(rename = "abstract")]
    abstract_text: Option<String>,
    #[serde(rename = "URL")]
    url: Option<String>,
    #[serde(rename = "DOI")]
    doi: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossRefAuthor {
    given: Option<String>,
    family: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CrossRefDate {
    #[serde(rename = "date-parts")]
    date_parts: Option<Vec<Vec<Option<i32>>>>,
}

pub async fn fetch_by_doi(doi: &str) -> Result<Paper, String> {
    let url = format!("{CROSSREF_API}/{doi}");

    let client = crate::shared_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("CrossRef API returned status {}", resp.status()));
    }

    let data: CrossRefResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse CrossRef response: {e}"))?;

    let work = data.message;

    let title = work
        .title
        .and_then(|t| t.into_iter().next())
        .unwrap_or_default();

    let authors: Vec<String> = work
        .author
        .unwrap_or_default()
        .into_iter()
        .map(|a| match (a.given, a.family) {
            (Some(g), Some(f)) => format!("{g} {f}"),
            (None, Some(f)) => f,
            (Some(g), None) => g,
            (None, None) => String::new(),
        })
        .filter(|s| !s.is_empty())
        .collect();

    let year = work
        .published_print
        .or(work.published_online)
        .and_then(|d| d.date_parts)
        .and_then(|parts| parts.first().cloned())
        .and_then(|parts| parts.first().copied().flatten());

    let journal = work.container_title.and_then(|t| t.into_iter().next());

    // Strip HTML tags from abstract (CrossRef often returns JATS XML fragments)
    let abstract_text = work.abstract_text.map(|s| strip_html_tags(&s));

    Ok(Paper {
        title,
        authors,
        year,
        doi: Some(work.doi.unwrap_or_else(|| doi.to_string())),
        abstract_text,
        publication: Publication {
            journal,
            volume: work.volume,
            issue: work.issue,
            pages: work.page,
            publisher: work.publisher,
        },
        links: PaperLinks {
            url: work.url,
            ..Default::default()
        },
        ..Default::default()
    })
}

fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    result.trim().to_string()
}

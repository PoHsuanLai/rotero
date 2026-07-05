//! Resolves full metadata from CrossRef when a page carries a DOI, rather than
//! scraping the HTML. Mirrors Zotero's "DOI Content Negotiation" translator,
//! which content-negotiates CSL-JSON for a DOI; the CrossRef client is
//! equivalent for the fields the [`ZoteroItem`](crate::ZoteroItem) model holds.

use async_trait::async_trait;

use rotero_search::crossref;

use crate::item::ZoteroItem;

use super::{TranslationContext, Translator};

/// Resolves paper metadata for a page's DOI via CrossRef.
pub struct DoiContentNegotiation;

#[async_trait]
impl Translator for DoiContentNegotiation {
    fn id(&self) -> &'static str {
        "DOI Content Negotiation"
    }

    /// Above Embedded Metadata: a resolved DOI beats scraped meta tags.
    fn priority(&self) -> i32 {
        60
    }

    fn detect(&self, ctx: &TranslationContext) -> bool {
        find_doi(ctx).is_some()
    }

    async fn translate(
        &self,
        ctx: &TranslationContext,
    ) -> Result<Vec<ZoteroItem>, crate::TranslateError> {
        let doi = find_doi(ctx).ok_or(crate::TranslateError::NotApplicable)?;
        let paper = crossref::fetch_by_doi(&doi)
            .await
            .map_err(crate::TranslateError::Translation)?;
        Ok(vec![ZoteroItem::from_paper(paper)])
    }
}

/// Find a DOI for this page: first the URL, then a `citation_doi` meta tag.
fn find_doi(ctx: &TranslationContext) -> Option<String> {
    doi_from_url(&ctx.url).or_else(|| doi_from_body(&ctx.body))
}

/// Extract a DOI embedded in a URL (e.g. `https://doi.org/10.x/...` or
/// `...?doi=10.x/...`).
fn doi_from_url(url: &str) -> Option<String> {
    // doi.org path
    for marker in ["doi.org/", "doi=", "/doi/"] {
        if let Some(idx) = url.find(marker) {
            let rest = &url[idx + marker.len()..];
            if let Some(doi) = take_doi(rest) {
                return Some(doi);
            }
        }
    }
    None
}

/// Extract a DOI from a `<meta name="citation_doi" content="...">` tag without
/// a full HTML parse (cheap: a `detect()` should not pay for parsing).
fn doi_from_body(body: &str) -> Option<String> {
    let lower = body.to_ascii_lowercase();
    let idx = lower.find("citation_doi")?;
    // Find the content="..." after the name; scan a bounded window.
    let window = &body[idx..(idx + 400).min(body.len())];
    let content_idx = window.to_ascii_lowercase().find("content=")?;
    let after = &window[content_idx + "content=".len()..];
    let quote = after.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let val = &after[1..];
    let end = val.find(quote)?;
    take_doi(&val[..end])
}

/// Read a DOI (`10.NNNN/suffix`) from the start of `s`, trimming a scheme
/// prefix and stopping at a URL delimiter or trailing punctuation.
fn take_doi(s: &str) -> Option<String> {
    let s = s
        .trim_start_matches("https://doi.org/")
        .trim_start_matches("http://doi.org/")
        .trim();
    if !s.starts_with("10.") {
        return None;
    }
    let end = s
        .find(['\\', '"', '\'', '<', '>', ' ', '?', '#', '&'])
        .unwrap_or(s.len());
    let doi = s[..end].trim_end_matches(['.', ',', ';']);
    // A valid DOI has a "10.NNNN/" prefix then a suffix.
    let rest = &doi[3..];
    let (registrant, suffix) = rest.split_once('/')?;
    if registrant.len() >= 4 && registrant.bytes().all(|b| b.is_ascii_digit()) && !suffix.is_empty()
    {
        Some(doi.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_doi_from_doi_org_url() {
        assert_eq!(
            doi_from_url("https://doi.org/10.1038/nature12373"),
            Some("10.1038/nature12373".to_string())
        );
    }

    #[test]
    fn extracts_doi_from_query_param() {
        assert_eq!(
            doi_from_url("https://pub.example.com/article?doi=10.1234/abcd.ef&x=1"),
            Some("10.1234/abcd.ef".to_string())
        );
    }

    #[test]
    fn extracts_doi_from_publisher_doi_path() {
        assert_eq!(
            doi_from_url("https://journals.example.org/doi/10.1109/5.771073"),
            Some("10.1109/5.771073".to_string())
        );
    }

    #[test]
    fn rejects_non_doi_url() {
        assert_eq!(doi_from_url("https://example.com/some/article"), None);
        assert_eq!(doi_from_url("https://doi.org/not-a-doi"), None);
    }

    #[test]
    fn extracts_doi_from_citation_meta() {
        let body = r#"<html><head>
            <meta name="citation_doi" content="10.5555/3295222.3295349">
        </head></html>"#;
        assert_eq!(
            doi_from_body(body),
            Some("10.5555/3295222.3295349".to_string())
        );
    }

    #[test]
    fn strips_trailing_punctuation() {
        assert_eq!(take_doi("10.1000/xyz."), Some("10.1000/xyz".to_string()));
    }
}

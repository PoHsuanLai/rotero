use std::sync::LazyLock;

use regex::Regex;
use rotero_models::PaperId;

static DOI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(10\.\d{4,}/[^\s]+)").unwrap());

static ARXIV_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"arXiv:(\d{4}\.\d{4,5}(?:v\d+)?|[a-z-]+/\d{7}(?:v\d+)?)").unwrap()
});

pub fn extract_doi(text: &str) -> Option<String> {
    DOI_RE.find(text).map(|m| {
        m.as_str()
            .trim_end_matches(['.', ',', ';', ')', ']', '}'])
            .to_string()
    })
}

pub fn extract_arxiv_id(text: &str) -> Option<String> {
    ARXIV_RE.captures(text).map(|c| {
        let id = c.get(1).unwrap().as_str();
        if let Some(pos) = id.rfind('v')
            && id[pos + 1..].chars().all(|c| c.is_ascii_digit())
        {
            return id[..pos].to_string();
        }
        id.to_string()
    })
}

/// Extract the best paper identifier from raw text.
/// Prefers DOI over arXiv (DOI is more universal).
pub fn extract_paper_id(text: &str) -> Option<PaperId> {
    if let Some(doi) = extract_doi(text) {
        PaperId::parse(&doi)
    } else {
        extract_arxiv_id(text).map(PaperId::ArXiv)
    }
}

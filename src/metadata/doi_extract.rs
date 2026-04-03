use std::sync::LazyLock;

use regex::Regex;

static DOI_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(10\.\d{4,}/[^\s]+)").unwrap());

static ARXIV_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Matches both old format (hep-ph/0601234) and new format (1802.06070)
    Regex::new(r"arXiv:(\d{4}\.\d{4,5}(?:v\d+)?|[a-z-]+/\d{7}(?:v\d+)?)").unwrap()
});

/// Extract the first DOI found in the given text.
pub fn extract_doi(text: &str) -> Option<String> {
    DOI_RE.find(text).map(|m| {
        m.as_str()
            .trim_end_matches(['.', ',', ';', ')', ']', '}'])
            .to_string()
    })
}

/// Extract the first arXiv ID found in the given text (e.g. "1802.06070" or "1802.06070v6").
pub fn extract_arxiv_id(text: &str) -> Option<String> {
    ARXIV_RE.captures(text).map(|c| {
        let id = c.get(1).unwrap().as_str();
        // Strip version suffix for API lookup
        if let Some(pos) = id.rfind('v')
            && id[pos + 1..].chars().all(|c| c.is_ascii_digit()) {
                return id[..pos].to_string();
            }
        id.to_string()
    })
}

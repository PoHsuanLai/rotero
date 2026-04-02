use super::crossref::FetchedMetadata;

const ARXIV_API: &str = "https://export.arxiv.org/api/query";

/// Fetch metadata from the arXiv API using an arXiv ID (e.g. "1802.06070").
pub async fn fetch_by_arxiv_id(arxiv_id: &str) -> Result<FetchedMetadata, String> {
    let url = format!("{ARXIV_API}?id_list={arxiv_id}");

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("arXiv request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("arXiv API returned status {}", resp.status()));
    }

    let body = resp.text().await.map_err(|e| format!("Failed to read arXiv response: {e}"))?;
    parse_arxiv_atom(&body, arxiv_id)
}

fn parse_arxiv_atom(xml: &str, arxiv_id: &str) -> Result<FetchedMetadata, String> {
    // Simple XML parsing — arXiv returns Atom XML
    let entry_start = xml.find("<entry>").ok_or("No entry in arXiv response")?;
    let entry_end = xml.find("</entry>").ok_or("Malformed arXiv response")?;
    let entry = &xml[entry_start..entry_end];

    let title = extract_tag(entry, "title")
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    if title.is_empty() || title.contains("Error") {
        return Err(format!("arXiv paper not found: {arxiv_id}"));
    }

    let abstract_text = extract_tag(entry, "summary")
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "));

    // Extract authors: <author><name>...</name></author>
    let authors: Vec<String> = entry
        .match_indices("<author>")
        .filter_map(|(start, _)| {
            let sub = &entry[start..];
            extract_tag(sub, "name")
        })
        .collect();

    // Extract year from <published>2018-02-16T...</published>
    let year = extract_tag(entry, "published")
        .and_then(|s| s.get(..4)?.parse::<i32>().ok());

    Ok(FetchedMetadata {
        title,
        authors,
        year,
        journal: Some("arXiv".to_string()),
        volume: None,
        issue: None,
        pages: None,
        publisher: None,
        abstract_text,
        url: Some(format!("https://arxiv.org/abs/{arxiv_id}")),
        doi: format!("arXiv:{arxiv_id}"),
    })
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let content = xml[start..end].trim();
    if content.is_empty() {
        None
    } else {
        Some(content.to_string())
    }
}

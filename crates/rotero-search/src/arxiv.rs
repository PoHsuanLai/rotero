use rotero_models::{Paper, PaperLinks, Publication};

const ARXIV_API: &str = "https://export.arxiv.org/api/query";

/// Searches the arXiv API for papers matching the given query string.
pub async fn search_papers(query: &str, limit: usize) -> Result<Vec<Paper>, String> {
    let url = format!(
        "{ARXIV_API}?search_query=all:{}&start=0&max_results={limit}",
        urlencoding::encode(query)
    );

    let client = crate::shared_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("arXiv request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("arXiv API returned status {}", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read arXiv response: {e}"))?;

    parse_arxiv_entries(&body)
}

fn parse_arxiv_entries(xml: &str) -> Result<Vec<Paper>, String> {
    let mut results = Vec::new();
    let mut search_from = 0;

    while let Some(start) = xml[search_from..].find("<entry>") {
        let abs_start = search_from + start;
        let Some(end) = xml[abs_start..].find("</entry>") else {
            break;
        };
        let abs_end = abs_start + end + "</entry>".len();
        let entry = &xml[abs_start..abs_end];

        let arxiv_id = extract_tag(entry, "id")
            .and_then(|url| url.rsplit('/').next().map(|s| s.to_string()))
            .unwrap_or_default();

        // Remove version suffix (e.g. "1802.06070v2" -> "1802.06070")
        let arxiv_id = arxiv_id.split('v').next().unwrap_or(&arxiv_id);

        if let Ok(paper) = parse_arxiv_atom(entry, arxiv_id) {
            results.push(paper);
        }

        search_from = abs_end;
    }

    Ok(results)
}

/// Fetch metadata from the arXiv API using an arXiv ID (e.g. "1802.06070").
pub async fn fetch_by_arxiv_id(arxiv_id: &str) -> Result<Paper, String> {
    let url = format!("{ARXIV_API}?id_list={}", urlencoding::encode(arxiv_id));

    let client = crate::shared_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("arXiv request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("arXiv API returned status {}", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read arXiv response: {e}"))?;
    parse_arxiv_atom(&body, arxiv_id)
}

fn parse_arxiv_atom(xml: &str, arxiv_id: &str) -> Result<Paper, String> {
    let entry_start = xml.find("<entry>").ok_or("No entry in arXiv response")?;
    let entry_end = xml.find("</entry>").ok_or("Malformed arXiv response")?;
    let entry = &xml[entry_start..entry_end];

    let title = extract_tag(entry, "title")
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();

    if title.is_empty() || title.contains("Error") {
        return Err(format!("arXiv paper not found: {arxiv_id}"));
    }

    let abstract_text =
        extract_tag(entry, "summary").map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "));

    let authors: Vec<String> = entry
        .match_indices("<author>")
        .filter_map(|(start, _)| {
            let sub = &entry[start..];
            extract_tag(sub, "name")
        })
        .collect();

    let year = extract_tag(entry, "published").and_then(|s| s.get(..4)?.parse::<i32>().ok());

    Ok(Paper {
        title,
        authors,
        year,
        doi: Some(format!("arXiv:{arxiv_id}")),
        abstract_text,
        publication: Publication {
            journal: Some("arXiv".to_string()),
            ..Default::default()
        },
        links: PaperLinks {
            url: Some(format!("https://arxiv.org/abs/{arxiv_id}")),
            ..Default::default()
        },
        ..Default::default()
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

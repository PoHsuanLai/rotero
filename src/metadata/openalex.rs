use serde::Deserialize;

use super::crossref::FetchedMetadata;

const OPENALEX_API: &str = "https://api.openalex.org/works";

#[derive(Debug, Deserialize)]
struct OpenAlexWork {
    title: Option<String>,
    doi: Option<String>,
    publication_year: Option<i32>,
    #[serde(rename = "primary_location")]
    primary_location: Option<OpenAlexLocation>,
    authorships: Option<Vec<OpenAlexAuthorship>>,
    #[serde(rename = "abstract_inverted_index")]
    abstract_inverted_index: Option<serde_json::Value>,
    cited_by_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexLocation {
    source: Option<OpenAlexSource>,
    #[allow(dead_code)]
    pdf_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexSource {
    display_name: Option<String>,
    host_organization_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexAuthorship {
    author: Option<OpenAlexAuthor>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexAuthor {
    display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexSearchResponse {
    results: Option<Vec<OpenAlexWork>>,
}

/// Fetch metadata from OpenAlex by DOI.
pub async fn fetch_by_doi(doi: &str) -> Result<FetchedMetadata, String> {
    let url = format!("{OPENALEX_API}/https://doi.org/{doi}");
    let work = fetch_work(&url).await?;
    work_to_metadata(work, doi)
}

/// Search OpenAlex by title and return the best match.
pub async fn search_by_title(title: &str) -> Result<FetchedMetadata, String> {
    let url = format!(
        "{OPENALEX_API}?search={}&per_page=1",
        urlencoding::encode(title)
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", "Rotero/0.1.0 (mailto:rotero@example.com)")
        .send()
        .await
        .map_err(|e| format!("OpenAlex request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("OpenAlex API returned status {}", resp.status()));
    }

    let data: OpenAlexSearchResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse OpenAlex response: {e}"))?;

    let work = data
        .results
        .and_then(|mut r| {
            if r.is_empty() {
                None
            } else {
                Some(r.remove(0))
            }
        })
        .ok_or_else(|| "No results found on OpenAlex".to_string())?;

    let doi = work
        .doi
        .as_deref()
        .unwrap_or("")
        .replace("https://doi.org/", "");
    work_to_metadata(work, &doi)
}

async fn fetch_work(url: &str) -> Result<OpenAlexWork, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .header("User-Agent", "Rotero/0.1.0 (mailto:rotero@example.com)")
        .send()
        .await
        .map_err(|e| format!("OpenAlex request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("OpenAlex API returned status {}", resp.status()));
    }

    resp.json()
        .await
        .map_err(|e| format!("Failed to parse OpenAlex response: {e}"))
}

fn work_to_metadata(work: OpenAlexWork, doi: &str) -> Result<FetchedMetadata, String> {
    let title = work.title.unwrap_or_default();
    if title.is_empty() {
        return Err("Empty title from OpenAlex".to_string());
    }

    let authors: Vec<String> = work
        .authorships
        .unwrap_or_default()
        .into_iter()
        .filter_map(|a| a.author?.display_name)
        .collect();

    let journal = work
        .primary_location
        .as_ref()
        .and_then(|l| l.source.as_ref())
        .and_then(|s| s.display_name.clone());

    let publisher = work
        .primary_location
        .as_ref()
        .and_then(|l| l.source.as_ref())
        .and_then(|s| s.host_organization_name.clone());

    let abstract_text = work
        .abstract_inverted_index
        .and_then(|idx| reconstruct_abstract(&idx));

    Ok(FetchedMetadata {
        title,
        authors,
        year: work.publication_year,
        journal,
        volume: None,
        issue: None,
        pages: None,
        publisher,
        abstract_text,
        url: None,
        doi: if doi.is_empty() {
            String::new()
        } else {
            doi.to_string()
        },
        citation_count: work.cited_by_count,
    })
}

/// Reconstruct abstract from OpenAlex's inverted index format.
/// The index maps words to their positions in the abstract.
fn reconstruct_abstract(index: &serde_json::Value) -> Option<String> {
    let obj = index.as_object()?;
    let mut words: Vec<(usize, &str)> = Vec::new();

    for (word, positions) in obj {
        if let Some(arr) = positions.as_array() {
            for pos in arr {
                if let Some(i) = pos.as_u64() {
                    words.push((i as usize, word.as_str()));
                }
            }
        }
    }

    if words.is_empty() {
        return None;
    }

    words.sort_by_key(|(i, _)| *i);
    Some(
        words
            .into_iter()
            .map(|(_, w)| w)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

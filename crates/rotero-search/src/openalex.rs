use serde::Deserialize;

use crate::FetchedMetadata;

const OPENALEX_API: &str = "https://api.openalex.org/works";
const OPENALEX_AUTOCOMPLETE: &str = "https://api.openalex.org/autocomplete/works";

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
    let results = search_papers(title, 1).await?;
    results
        .into_iter()
        .next()
        .ok_or_else(|| "No results found on OpenAlex".to_string())
}

/// Search OpenAlex and return up to `limit` results.
pub async fn search_papers(query: &str, limit: usize) -> Result<Vec<FetchedMetadata>, String> {
    let url = format!(
        "{OPENALEX_API}?search={}&per_page={limit}",
        urlencoding::encode(query)
    );

    let client = crate::shared_client();
    let resp = client
        .get(&url)
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

    let works = data.results.unwrap_or_default();
    let mut results = Vec::new();
    for work in works {
        let doi = work
            .doi
            .as_deref()
            .unwrap_or("")
            .replace("https://doi.org/", "");
        if let Ok(meta) = work_to_metadata(work, &doi) {
            results.push(meta);
        }
    }
    Ok(results)
}

/// Fast autocomplete search — returns lightweight results (~50-100ms).
/// Use this for live type-ahead, then fetch full details on import.
pub async fn autocomplete(query: &str) -> Result<Vec<FetchedMetadata>, String> {
    let url = format!("{OPENALEX_AUTOCOMPLETE}?q={}", urlencoding::encode(query));

    let client = crate::shared_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("OpenAlex autocomplete failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "OpenAlex autocomplete returned status {}",
            resp.status()
        ));
    }

    let data: AutocompleteResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse autocomplete response: {e}"))?;

    let mut results = Vec::new();
    for item in data.results.unwrap_or_default() {
        let title = item.display_name.unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let doi = item
            .external_id
            .as_deref()
            .unwrap_or("")
            .replace("https://doi.org/", "");
        results.push(FetchedMetadata {
            title,
            authors: Vec::new(),
            year: None,
            journal: item.hint.clone(),
            volume: None,
            issue: None,
            pages: None,
            publisher: None,
            abstract_text: None,
            url: None,
            doi,
            citation_count: item.cited_by_count,
        });
    }
    Ok(results)
}

#[derive(Debug, Deserialize)]
struct AutocompleteResponse {
    results: Option<Vec<AutocompleteItem>>,
}

#[derive(Debug, Deserialize)]
struct AutocompleteItem {
    display_name: Option<String>,
    external_id: Option<String>,
    cited_by_count: Option<i64>,
    hint: Option<String>,
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

use rotero_models::{CitationInfo, Paper, Publication};
use serde::Deserialize;

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
    open_access: Option<OpenAlexOA>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexOA {
    oa_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAlexLocation {
    source: Option<OpenAlexSource>,
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

pub async fn fetch_by_doi(doi: &str) -> Result<Paper, String> {
    let url = format!("{OPENALEX_API}/https://doi.org/{doi}");
    let work = fetch_work(&url).await?;
    work_to_paper(work, doi)
}

pub async fn search_by_title(title: &str) -> Result<Paper, String> {
    let results = search_papers(title, 1).await?;
    results
        .into_iter()
        .next()
        .ok_or_else(|| "No results found on OpenAlex".to_string())
}

pub async fn search_papers(query: &str, limit: usize) -> Result<Vec<Paper>, String> {
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

    let results = data
        .results
        .unwrap_or_default()
        .into_iter()
        .filter_map(|work| {
            let doi = work.doi.as_deref().unwrap_or("").replace("https://doi.org/", "");
            work_to_paper(work, &doi).ok()
        })
        .collect();
    Ok(results)
}

/// Find open-access PDF URLs via OpenAlex.
/// Returns candidate URLs ordered by likelihood of being a direct PDF link.
/// Tries DOI lookup first (if provided), then falls back to title search.
pub async fn find_oa_pdf(doi: Option<&str>, title: &str) -> Result<Vec<String>, String> {
    let mut urls = Vec::new();

    let work = if let Some(doi) = doi {
        let url = format!("{OPENALEX_API}/https://doi.org/{doi}");
        fetch_work(&url).await.ok()
    } else {
        None
    };

    // Fall back to title search if DOI lookup didn't return a work
    let work = match work {
        Some(w) => Some(w),
        None => {
            let url = format!(
                "{OPENALEX_API}?search={}&per_page=1",
                urlencoding::encode(title)
            );
            let client = crate::shared_client();
            if let Ok(resp) = client.get(&url).send().await
                && resp.status().is_success()
            {
                let data: OpenAlexSearchResponse = resp
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse OpenAlex response: {e}"))?;
                data.results.and_then(|r| r.into_iter().next())
            } else {
                None
            }
        }
    };

    if let Some(work) = work {
        // pdf_url first — more likely to be a direct PDF link
        if let Some(ref loc) = work.primary_location
            && let Some(ref pdf_url) = loc.pdf_url
        {
            urls.push(pdf_url.clone());
        }
        if let Some(oa_url) = work.open_access.and_then(|oa| oa.oa_url)
            && !urls.contains(&oa_url)
        {
            urls.push(oa_url);
        }
    }

    Ok(urls)
}

/// Fast autocomplete search — returns lightweight results (~50-100ms).
/// Use this for live type-ahead, then fetch full details on import.
pub async fn autocomplete(query: &str) -> Result<Vec<Paper>, String> {
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
        results.push(Paper {
            title,
            doi: if doi.is_empty() { None } else { Some(doi) },
            publication: Publication {
                journal: item.hint.clone(),
                ..Default::default()
            },
            citation: CitationInfo {
                citation_count: item.cited_by_count,
                ..Default::default()
            },
            ..Default::default()
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
    let resp = crate::shared_client()
        .get(url)
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

fn work_to_paper(work: OpenAlexWork, doi: &str) -> Result<Paper, String> {
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

    Ok(Paper {
        title,
        authors,
        year: work.publication_year,
        doi: if doi.is_empty() {
            None
        } else {
            Some(doi.to_string())
        },
        abstract_text,
        publication: Publication {
            journal,
            publisher,
            ..Default::default()
        },
        citation: CitationInfo {
            citation_count: work.cited_by_count,
            ..Default::default()
        },
        ..Default::default()
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

use std::fmt;

use rotero_db::Database;

#[derive(Debug)]
pub enum PdfDownloadError {
    NoUrls,
    AllFailed(String),
}

impl fmt::Display for PdfDownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoUrls => write!(f, "No OA version found"),
            Self::AllFailed(msg) => write!(f, "{msg}"),
        }
    }
}

/// Resolve candidate PDF URLs from the Zotero translation server, OpenAlex, and Unpaywall.
pub async fn resolve_pdf_urls(doi: Option<&str>, title: &str) -> Vec<String> {
    tracing::info!("resolve_pdf_urls: doi={:?}, title={:?}", doi, title);
    let mut urls = Vec::new();

    // Try Zotero translation server first — it has site-specific scrapers that
    // return direct PDF links far more reliably than OA metadata APIs.
    if let Some(doi) = doi {
        match zotero_pdf_urls(doi).await {
            Ok(zotero_urls) => {
                tracing::info!("Zotero translation server returned {} URLs: {:?}", zotero_urls.len(), zotero_urls);
                urls.extend(zotero_urls);
            }
            Err(e) => tracing::warn!("Zotero translation server failed: {e}"),
        }
    } else {
        tracing::info!("Skipping Zotero translation server (no DOI)");
    }

    // OpenAlex as secondary source
    match rotero_search::openalex::find_oa_pdf(doi, title).await {
        Ok(oa_urls) => {
            tracing::info!("OpenAlex returned {} URLs: {:?}", oa_urls.len(), oa_urls);
            for url in oa_urls {
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }
        Err(e) => tracing::warn!("OpenAlex find_oa_pdf failed: {e}"),
    }

    // Semantic Scholar — often has OA links for conference papers that OpenAlex misses
    match rotero_search::semantic_scholar::find_oa_pdf(doi, title).await {
        Ok(Some(url)) => {
            tracing::info!("Semantic Scholar returned OA PDF: {url}");
            if !urls.contains(&url) {
                urls.push(url);
            }
        }
        Ok(None) => tracing::info!("Semantic Scholar returned no OA PDF"),
        Err(e) => tracing::warn!("Semantic Scholar find_oa_pdf failed: {e}"),
    }

    // Unpaywall as final fallback
    if let Some(doi) = doi {
        match rotero_search::unpaywall::fetch_oa_url(doi).await {
            Ok(Some(url)) => {
                tracing::info!("Unpaywall returned OA URL: {url}");
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
            Ok(None) => tracing::info!("Unpaywall returned no OA URL"),
            Err(e) => tracing::warn!("Unpaywall failed: {e}"),
        }
    } else {
        tracing::info!("Skipping Unpaywall (no DOI)");
    }

    tracing::info!("resolve_pdf_urls: final candidate URLs ({} total): {:?}", urls.len(), urls);
    urls
}

/// Query the local Zotero translation server for PDF attachment URLs.
async fn zotero_pdf_urls(doi: &str) -> Result<Vec<String>, String> {
    let client = reqwest::Client::new();
    let resp = client
        .post("http://127.0.0.1:1969/search")
        .header("Content-Type", "text/plain")
        .body(doi.to_string())
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Translation server request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Translation server HTTP {}", resp.status()));
    }

    let items: Vec<rotero_translate::ZoteroItem> = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse translation server response: {e}"))?;

    let urls: Vec<String> = items
        .iter()
        .filter_map(|item| item.pdf_url())
        .collect();

    Ok(urls)
}

/// Download a PDF from the first working URL and save it to the library.
/// Returns the relative path within the papers directory.
pub async fn download_and_save_pdf(
    db: &Database,
    urls: &[String],
    title: &str,
    first_author: Option<&str>,
    year: Option<i32>,
) -> Result<String, PdfDownloadError> {
    if urls.is_empty() {
        return Err(PdfDownloadError::NoUrls);
    }

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (compatible; Rotero/0.1)")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| PdfDownloadError::AllFailed(format!("HTTP client error: {e}")))?;

    let mut last_error = String::new();

    for url in urls {
        tracing::info!("Trying PDF download from: {url}");
        match try_download_pdf(&client, url).await {
            Ok(bytes) => {
                tracing::info!("PDF download succeeded from {url} ({} bytes)", bytes.len());
                return db
                    .import_pdf_bytes(&bytes, title, first_author, year)
                    .map_err(|e| PdfDownloadError::AllFailed(format!("Save failed: {e}")));
            }
            Err(e) => {
                tracing::warn!("PDF download failed for {url}: {e}");
                last_error = e;
            }
        }
    }

    Err(PdfDownloadError::AllFailed(last_error))
}

/// Try to download a PDF from a single URL. Returns the raw bytes on success.
async fn try_download_pdf(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let is_html = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.contains("text/html"));

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Download failed: {e}"))?;

    if is_html || !bytes.starts_with(b"%PDF") {
        return Err("OA link did not return a PDF".to_string());
    }

    Ok(bytes.to_vec())
}

/// Resolve URLs and download in one call.
pub async fn find_and_download_pdf(
    db: &Database,
    doi: Option<&str>,
    title: &str,
    first_author: Option<&str>,
    year: Option<i32>,
) -> Result<String, PdfDownloadError> {
    let urls = resolve_pdf_urls(doi, title).await;
    download_and_save_pdf(db, &urls, title, first_author, year).await
}

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

/// Resolve candidate PDF URLs from OpenAlex and Unpaywall.
pub async fn resolve_pdf_urls(doi: Option<&str>, title: &str) -> Vec<String> {
    let mut urls = rotero_search::openalex::find_oa_pdf(doi, title)
        .await
        .unwrap_or_default();

    // Try Unpaywall as fallback if we have a DOI
    if let Some(doi) = doi
        && let Ok(Some(url)) = rotero_search::unpaywall::fetch_oa_url(doi).await
        && !urls.contains(&url)
    {
        urls.push(url);
    }

    urls
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
        match try_download_pdf(&client, url).await {
            Ok(bytes) => {
                return db
                    .import_pdf_bytes(&bytes, title, first_author, year)
                    .map_err(|e| PdfDownloadError::AllFailed(format!("Save failed: {e}")));
            }
            Err(e) => {
                tracing::debug!("PDF download failed for {url}: {e}");
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

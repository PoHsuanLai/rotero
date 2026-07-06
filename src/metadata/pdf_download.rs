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

/// Resolve candidate PDF URLs for a paper, in priority order: a direct PDF link
/// the paper already carries (e.g. arXiv's `pdf_url`, an OA hit's location),
/// then the in-process translators (which often surface a direct publisher PDF
/// link), then the open-access metadata APIs (OpenAlex, Semantic Scholar,
/// Unpaywall).
///
/// The direct link matters most for arXiv results: their DOI is a pseudo-DOI
/// (`arXiv:ID`) the OA APIs don't resolve, so without it the download finds
/// nothing even though a public PDF exists.
pub async fn resolve_pdf_urls(
    direct_url: Option<&str>,
    doi: Option<&str>,
    title: &str,
) -> Vec<String> {
    tracing::info!(
        "resolve_pdf_urls: direct_url={:?}, doi={:?}, title={:?}",
        direct_url,
        doi,
        title
    );
    let mut urls = Vec::new();

    if let Some(url) = direct_url
        && !url.is_empty()
    {
        urls.push(url.to_string());
    }

    // Translator scrape: resolve the DOI to its publisher page and let the site
    // translator surface a direct PDF attachment (arXiv "Preprint PDF", a Nature
    // full-text link, …). Runs against the local connector so it reuses the same
    // registry the browser extension uses. Best for OA-friendly sites; gated
    // publishers block the server-side fetch and simply return nothing, falling
    // through to the OA APIs below.
    #[cfg(feature = "desktop")]
    if let Some(doi) = doi {
        match translator_pdf_urls(doi).await {
            Ok(scraped) if !scraped.is_empty() => {
                tracing::info!("translator scrape returned {} URL(s)", scraped.len());
                for url in scraped {
                    if !urls.contains(&url) {
                        urls.push(url);
                    }
                }
            }
            Ok(_) => tracing::info!("translator scrape returned no PDF URL"),
            Err(e) => tracing::debug!("translator scrape unavailable: {e}"),
        }
    }

    // OpenAlex
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

    tracing::info!(
        "resolve_pdf_urls: final candidate URLs ({} total): {:?}",
        urls.len(),
        urls
    );
    urls
}

/// Ask the local connector to translate the DOI's publisher page and return any
/// direct PDF URL its site translator surfaced. Uses `/api/scrape` so it shares
/// the registry (and telemetry) with the browser extension. Errors — including
/// the connector not running — are non-fatal: the caller falls through to the
/// OA APIs.
#[cfg(feature = "desktop")]
async fn translator_pdf_urls(doi: &str) -> Result<Vec<String>, String> {
    #[derive(serde::Serialize)]
    struct ScrapeReq {
        url: String,
    }
    #[derive(serde::Deserialize)]
    struct ScrapeResp {
        success: bool,
        metadata: Option<MetaOut>,
    }
    #[derive(serde::Deserialize)]
    struct MetaOut {
        pdf_url: Option<String>,
    }

    let endpoint = format!(
        "http://127.0.0.1:{}/api/scrape",
        rotero_connector::CONNECTOR_PORT
    );
    let resp = reqwest::Client::new()
        .post(&endpoint)
        .json(&ScrapeReq {
            url: format!("https://doi.org/{doi}"),
        })
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("connector request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("connector HTTP {}", resp.status()));
    }

    let body: ScrapeResp = resp
        .json()
        .await
        .map_err(|e| format!("connector response parse failed: {e}"))?;

    Ok(body
        .success
        .then(|| body.metadata.and_then(|m| m.pdf_url))
        .flatten()
        .into_iter()
        .collect())
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
///
/// `direct_url` is any PDF link the paper already carries (arXiv `pdf_url`, an
/// OA location); it is tried first before falling back to translator scraping
/// and the OA metadata APIs.
pub async fn find_and_download_pdf(
    db: &Database,
    direct_url: Option<&str>,
    doi: Option<&str>,
    title: &str,
    first_author: Option<&str>,
    year: Option<i32>,
) -> Result<String, PdfDownloadError> {
    let urls = resolve_pdf_urls(direct_url, doi, title).await;
    download_and_save_pdf(db, &urls, title, first_author, year).await
}

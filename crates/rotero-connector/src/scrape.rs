use rotero_models::Paper;

/// Fetches the given URL and extracts paper metadata from its HTML.
///
/// The HTML-meta/JSON-LD extraction lives in `rotero_translate::html_meta` so it
/// can be shared with the native translator registry; this function is the
/// fetch wrapper (SSRF guard, redirect policy, User-Agent).
pub async fn scrape_url(url: &str) -> Result<Paper, String> {
    // Validate URL scheme to prevent SSRF (file://, internal networks, etc.)
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Unsupported URL scheme: {scheme}")),
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let resp = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; Rotero/0.1; +https://github.com/rotero)",
        )
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }

    let final_url = resp.url().to_string();
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    let mut paper = rotero_translate::extract_from_html(&html);
    paper.links.url = Some(final_url);
    Ok(paper)
}

use serde::Deserialize;

/// Base URL for this provider. `ROTERO_UNPAYWALL_API` overrides it so tests can
/// point at a local stub; unset or empty uses the real endpoint.
fn unpaywall_api_url() -> String {
    crate::base_url("ROTERO_UNPAYWALL_API", "https://api.unpaywall.org/v2")
}
const EMAIL: &str = "rotero.app@proton.me";

#[derive(Debug, Deserialize)]
struct UnpaywallResponse {
    best_oa_location: Option<OaLocation>,
}

#[derive(Debug, Deserialize)]
struct OaLocation {
    url_for_pdf: Option<String>,
    url: Option<String>,
}

/// Check Unpaywall for an open-access PDF URL for the given DOI.
pub async fn fetch_oa_url(doi: &str) -> Result<Option<String>, String> {
    let unpaywall_api = unpaywall_api_url();
    let url = format!("{unpaywall_api}/{doi}?email={EMAIL}");

    let client = crate::shared_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Unpaywall request failed: {e}"))?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let data: UnpaywallResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse Unpaywall response: {e}"))?;

    Ok(data
        .best_oa_location
        .and_then(|loc| loc.url_for_pdf.or(loc.url)))
}

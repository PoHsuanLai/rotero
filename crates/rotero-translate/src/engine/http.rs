//! Blocking HTTP for the engine's `ZU.doGet` / `ZU.doPost` / `ZU.processDocuments`
//! host functions.
//!
//! Translators make HTTP requests via callback-style helpers. The engine runs
//! synchronously on a `spawn_blocking` thread (boa's `Context` is `!Send`), so
//! these use `reqwest`'s blocking client rather than the async one — the request
//! blocks the worker thread, and the JS shim invokes the translator's callback
//! with the body once it returns.

use std::sync::OnceLock;
use std::time::Duration;

/// A process-wide blocking HTTP client with a browser-like User-Agent and a
/// bounded redirect policy. Built once.
fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(10))
            .timeout(Duration::from_secs(30))
            .user_agent("Mozilla/5.0 (compatible; Rotero/0.1; +https://github.com/rotero)")
            .build()
            .unwrap_or_default()
    })
}

/// Reject non-HTTP(S) URLs before making a request (SSRF guard), mirroring the
/// async `fetch_context` scheme check.
fn scheme_ok(url: &str) -> bool {
    reqwest::Url::parse(url)
        .map(|u| matches!(u.scheme(), "http" | "https"))
        .unwrap_or(false)
}

/// Apply caller-supplied request headers (e.g. `Referer`, which several gated
/// publishers require on their citation endpoints). Malformed header names/values
/// are skipped rather than failing the whole request.
fn apply_headers(
    mut req: reqwest::blocking::RequestBuilder,
    headers: &[(String, String)],
) -> reqwest::blocking::RequestBuilder {
    use reqwest::header::{HeaderName, HeaderValue};
    for (name, value) in headers {
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            req = req.header(n, v);
        }
    }
    req
}

/// Blocking GET returning the response body, or `Err(message)` on any failure.
/// The engine surfaces the error to the translator as an empty result.
pub fn get(url: &str, headers: &[(String, String)]) -> Result<String, String> {
    if !scheme_ok(url) {
        return Err(format!("blocked non-http(s) URL: {url}"));
    }
    let resp = apply_headers(client().get(url), headers)
        .send()
        .map_err(|e| format!("GET {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("GET {url}: HTTP {}", resp.status()));
    }
    resp.text().map_err(|e| format!("GET {url} body: {e}"))
}

/// Blocking POST of `body` with an explicit `Content-Type`, returning the
/// response body.
pub fn post(
    url: &str,
    body: &str,
    content_type: &str,
    headers: &[(String, String)],
) -> Result<String, String> {
    if !scheme_ok(url) {
        return Err(format!("blocked non-http(s) URL: {url}"));
    }
    let req = client()
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, content_type)
        .body(body.to_string());
    let resp = apply_headers(req, headers)
        .send()
        .map_err(|e| format!("POST {url}: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("POST {url}: HTTP {}", resp.status()));
    }
    resp.text().map_err(|e| format!("POST {url} body: {e}"))
}

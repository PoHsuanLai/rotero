//! Provider behaviour against a local stub instead of the live APIs.
//!
//! These used to be untestable: the base URLs were constants, so any test would
//! have hit the real services — flaky when a provider throttles a shared CI IP,
//! and impossible offline. The `ROTERO_*_API` overrides exist for this.
//!
//! The overrides are process-wide environment variables, so everything runs in
//! one test rather than racing sibling tests that set the same variable.

use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Shorten the shared client's timeout before it is built.
///
/// The client is a `OnceLock`, so this has to happen before the first request
/// anywhere in the process — otherwise the give-up test waits out the real 20s.
fn use_short_timeout() {
    unsafe { std::env::set_var("ROTERO_HTTP_TIMEOUT_MS", "1500") };
}

/// Set a base-URL override for the duration of a call.
///
/// # Safety
/// Single-threaded by construction: this file holds one test.
fn set_base_url(var: &str, value: &str) {
    unsafe { std::env::set_var(var, value) };
}

fn clear_base_url(var: &str) {
    unsafe { std::env::remove_var(var) };
}

#[tokio::test]
async fn providers_read_their_base_url_from_the_environment() {
    use_short_timeout();

    // --- a well-formed response is parsed --------------------------------
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "results": [{
                "id": "https://openalex.org/W1",
                "title": "Stubbed Paper",
                "publication_year": 2020,
                "doi": "https://doi.org/10.1234/stub",
                "authorships": [],
                "cited_by_count": 7,
            }]
        })))
        .mount(&server)
        .await;

    set_base_url("ROTERO_OPENALEX_API", &format!("{}/works", server.uri()));
    let found = rotero_search::openalex::search_papers("anything", 5).await;
    clear_base_url("ROTERO_OPENALEX_API");

    let papers = found.expect("a stubbed 200 must parse");
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0].title, "Stubbed Paper");

    // The empty-override fallback is asserted directly on `base_url` in the
    // crate's own unit tests. Inferring it from a provider call's error string
    // could not work: reqwest reports "builder error", so the substring this
    // used to look for never appeared and the assertion always passed.

    // --- a server error surfaces rather than being read as no results ----
    let failing = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&failing)
        .await;

    set_base_url("ROTERO_OPENALEX_API", &format!("{}/works", failing.uri()));
    let result = rotero_search::openalex::search_papers("anything", 5).await;
    clear_base_url("ROTERO_OPENALEX_API");

    assert!(
        result.is_err(),
        "an HTTP 500 must be an error, not an empty result set"
    );
}

/// The shared client carries a total timeout.
///
/// Without one, a provider that accepts the connection and then stalls holds the
/// request open forever, which is what parked the import queue.
#[tokio::test]
async fn requests_do_not_hang_forever() {
    // Must precede the first request in this process: the client is built once.
    use_short_timeout();

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        // Far longer than the client's configured timeout.
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(120)))
        .mount(&server)
        .await;

    set_base_url("ROTERO_CROSSREF_API", &format!("{}/works", server.uri()));
    let started = std::time::Instant::now();
    let result = rotero_search::crossref::fetch_by_doi("10.1234/hangs").await;
    let elapsed = started.elapsed();
    clear_base_url("ROTERO_CROSSREF_API");

    assert!(result.is_err(), "a stalled request must fail, not hang");
    // Bounded by the client, well short of the stub's 120s delay. Before the
    // timeout was added this waited for the response indefinitely.
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "the client must give up on its own; took {elapsed:?}"
    );
}

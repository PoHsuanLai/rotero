//! Live coverage against Zotero's own bundled web `testCases`.
//!
//! Each upstream web translator ships a `testCases` array of `{ url, items }`
//! pairs — the authoritative fixtures Zotero's CI runs against the live sites.
//! This fetches each case's `url` through the registry (running the vendored
//! translator in-process) and diffs the emitted [`ZoteroItem`] against the
//! expected item, field by field.
//!
//! Unlike the offline import harness, these hit the network, so they are
//! `#[ignore]`d — run explicitly with:
//!
//! ```sh
//! cargo test -p rotero-translate --features translator-engine --test web_testcases -- --ignored --nocapture
//! ```
//!
//! Scope is a curated set of open, server-fetchable publishers. Gated sites
//! (IEEE, Wiley, …) need the user's browser session and can't be driven headless,
//! so they're intentionally excluded — their path is the browser-proxied fetch,
//! exercised through the connector, not here.
#![cfg(feature = "translator-engine")]

use std::path::PathBuf;

use rotero_translate::ZoteroItem;
use rotero_translate::translators::TranslatorRegistry;

/// Directory holding the vendored `zotero/translators` corpus.
fn translators_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/translators")
}

/// One Zotero web test case: the page URL and the expected items. `items` is a
/// JSON array for single/multi-item pages, or the string `"multiple"` for a
/// selection page (which this harness skips — there's no single item to diff).
#[derive(serde::Deserialize)]
struct WebCase {
    #[serde(rename = "type")]
    kind: String,
    // Optional: non-web cases (import/search) in the same array carry no `url`,
    // and are filtered out after parsing.
    #[serde(default)]
    url: String,
    #[serde(default)]
    items: serde_json::Value,
}

/// The expected-item fields we diff. A web translator can populate far more than
/// the import path, so this matches on the core bibliographic set.
#[derive(serde::Deserialize, Default)]
struct ExpectedItem {
    #[serde(default)]
    title: String,
    #[serde(default)]
    creators: Vec<ExpectedCreator>,
    #[serde(default, rename = "DOI")]
    doi: String,
    #[serde(default)]
    date: String,
    #[serde(default, rename = "publicationTitle")]
    publication_title: String,
    #[serde(default)]
    volume: String,
    #[serde(default)]
    issue: String,
    #[serde(default)]
    pages: String,
    #[serde(default)]
    publisher: String,
}

#[derive(serde::Deserialize)]
struct ExpectedCreator {
    #[serde(default, rename = "lastName")]
    last_name: String,
    #[serde(default, rename = "firstName")]
    first_name: String,
    #[serde(default)]
    name: String,
}

impl ExpectedCreator {
    /// "First Last", or the single-field `name` for institutional authors.
    fn display(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if self.first_name.is_empty() {
            self.last_name.clone()
        } else {
            format!("{} {}", self.first_name, self.last_name)
        }
    }
}

/// Extract the `web`-type test cases (with a real item array — not `"multiple"`)
/// from a translator file.
fn web_cases(translator_file: &str) -> Vec<WebCase> {
    let path = translators_dir().join(translator_file);
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let begin = src
        .find("var testCases")
        .and_then(|i| src[i..].find('[').map(|j| i + j))
        .expect("testCases array start");
    let end_marker = src.find("/** END TEST CASES **/").unwrap_or(src.len());
    let close = src[..end_marker].rfind(']').expect("testCases array end");
    let json = &src[begin..=close];

    let cases: Vec<WebCase> = serde_json::from_str(json)
        .unwrap_or_else(|e| panic!("parse {translator_file} testCases: {e}"));
    cases
        .into_iter()
        .filter(|c| c.kind == "web" && !c.url.is_empty() && c.items.is_array())
        .collect()
}

/// Normalize a string for comparison: trim, collapse internal whitespace.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Case-insensitive DOI compare (Zotero lower-cases; some sites don't).
fn doi_eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Extract the 4-digit year from a Zotero `date` string.
fn year_of(date: &str) -> Option<&str> {
    date.split(|c: char| !c.is_ascii_digit())
        .find(|t| t.len() == 4)
}

/// Compare one emitted [`ZoteroItem`] against a Zotero expected item over the
/// diffed fields, returning `field: expected → got` messages for each mismatch.
fn compare(expected: &ExpectedItem, got: &ZoteroItem) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |field: &str, exp: &str, got: &str| {
        if !exp.is_empty() && norm(exp) != norm(got) {
            out.push(format!("{field}: {:?} → {:?}", norm(exp), norm(got)));
        }
    };

    check("title", &expected.title, &got.title);
    check(
        "publicationTitle",
        &expected.publication_title,
        &got.publication_title,
    );
    check("volume", &expected.volume, &got.volume);
    check("issue", &expected.issue, &got.issue);
    check("pages", &expected.pages, &got.pages);
    check("publisher", &expected.publisher, &got.publisher);

    if !expected.doi.is_empty() && !doi_eq(&expected.doi, &got.doi) {
        out.push(format!("DOI: {:?} → {:?}", expected.doi, got.doi));
    }

    // Year (from the expected `date`); the emitted date may be a full date.
    if let Some(exp_year) = year_of(&expected.date) {
        let got_year = year_of(&got.date).unwrap_or("");
        if exp_year != got_year {
            out.push(format!("year: {exp_year:?} → {got_year:?}"));
        }
    }

    // Authors: ordered display-name list.
    let exp_authors: Vec<String> = expected
        .creators
        .iter()
        .filter(|c| !c.display().is_empty())
        .map(|c| norm(&c.display()))
        .collect();
    if !exp_authors.is_empty() {
        let got_authors: Vec<String> = got
            .creators
            .iter()
            .map(|c| {
                let d = if !c.name.is_empty() {
                    c.name.clone()
                } else if c.first_name.is_empty() {
                    c.last_name.clone()
                } else {
                    format!("{} {}", c.first_name, c.last_name)
                };
                norm(&d)
            })
            .collect();
        if exp_authors != got_authors {
            out.push(format!(
                "creators: {:?} → {:?}",
                exp_authors.join(" | "),
                got_authors.join(" | ")
            ));
        }
    }

    out
}

/// Fetch and translate every single-item web case in `translator_file`, diff each
/// against its first expected item, and report the pass count. Network failures
/// (a moved page, a site now behind a wall) are reported but don't panic — this is
/// a coverage measurement over live sites, which drift.
async fn measure(translator_file: &str) -> (usize, usize, Vec<String>) {
    let registry = TranslatorRegistry::with_builtins();
    let cases = web_cases(translator_file);

    let mut passed = 0;
    let mut notes = Vec::new();

    for (i, case) in cases.iter().enumerate() {
        let expected: Vec<ExpectedItem> = match serde_json::from_value(case.items.clone()) {
            Ok(items) => items,
            Err(e) => {
                notes.push(format!("  case {i}: expected items parse failed: {e}"));
                continue;
            }
        };
        let Some(exp) = expected.first() else {
            continue;
        };

        match registry.translate_web(&case.url).await {
            Some(items) if !items.is_empty() => {
                let mismatches = compare(exp, &items[0]);
                if mismatches.is_empty() {
                    passed += 1;
                } else {
                    notes.push(format!("  case {i} ({}):", case.url));
                    for m in mismatches {
                        notes.push(format!("    {m}"));
                    }
                }
            }
            _ => notes.push(format!("  case {i} ({}): no item extracted", case.url)),
        }
    }

    (passed, cases.len(), notes)
}

/// Curated open, server-fetchable publishers with web test cases. Gated sites are
/// excluded (they need the browser session; see the module docs).
const PUBLISHERS: &[&str] = &[
    "arXiv.org.js",
    "PLoS Journals.js",
    "eLife.js",
    "Theory of Computing.js",
    "Frontiers.js",
    "Nature Publishing Group.js",
    "PubMed Central.js",
];

#[tokio::test]
#[ignore = "hits the live network; run with --ignored"]
async fn web_testcases_coverage_report() {
    let mut total_pass = 0;
    let mut total = 0;
    for &pubr in PUBLISHERS {
        let (passed, count, notes) = measure(pubr).await;
        total_pass += passed;
        total += count;
        let pct = (passed * 100).checked_div(count).unwrap_or(0);
        println!("{pubr}: {passed}/{count} single-item web cases pass ({pct}%)");
        for n in &notes {
            println!("{n}");
        }
    }
    println!("\nTOTAL: {total_pass}/{total} live web cases pass");

    // A soft floor so a wholesale breakage (fetch path down, corpus unloaded) is
    // caught, without pinning to flaky per-site live results.
    assert!(
        total_pass > 0,
        "no live web cases passed — the fetch/translate path is broken"
    );
}

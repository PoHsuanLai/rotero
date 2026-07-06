//! Offline coverage against Zotero's own bundled `testCases`.
//!
//! Each upstream import translator (BibTeX, RIS, MEDLINE/nbib, CSL JSON) ships a
//! `testCases` array of `{ input, items }` pairs — the authoritative fixtures
//! Zotero's CI runs. For the formats `rotero-bib` parses, this feeds each case's
//! inline `input` through our importer and diffs the result against the expected
//! `items`, field by field, over the subset of fields the flat `Paper` models.
//!
//! Fully offline (the input is inline, no network) and deterministic, so it runs
//! in CI. It reports a per-case pass/fail table and asserts a coverage floor, so
//! a regression that drops a field shows up as a shrinking pass count.

use std::path::PathBuf;

use rotero_models::Paper;

/// Directory holding the vendored `zotero/translators` corpus.
fn translators_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/translators")
}

/// One Zotero import test case: the inline source and the expected items.
#[derive(serde::Deserialize)]
struct TestCase {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    input: String,
    #[serde(default)]
    items: serde_json::Value,
}

/// The expected-item fields our flat model can represent. Everything else in the
/// Zotero item (accessDate, libraryCatalog, tags, notes, attachments, …) is out
/// of scope for this comparison.
#[derive(serde::Deserialize, Default)]
struct ExpectedItem {
    #[serde(default)]
    #[serde(rename = "itemType")]
    _item_type: String,
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
    #[serde(default)]
    #[serde(rename = "lastName")]
    last_name: String,
    #[serde(default)]
    #[serde(rename = "firstName")]
    first_name: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "creatorType")]
    creator_type: String,
}

impl ExpectedCreator {
    /// The creator rendered as our importers store authors: "First Last", or the
    /// single-field `name` for institutional authors.
    fn display(&self) -> String {
        if !self.name.is_empty() {
            self.name.clone()
        } else if self.first_name.is_empty() {
            self.last_name.clone()
        } else {
            format!("{} {}", self.first_name, self.last_name)
        }
    }

    /// Whether this is an author. The flat `Paper` models only an author list, so
    /// other creator roles (editors, translators, producers, …) are out of scope
    /// for the comparison. Zotero omits `creatorType` for authors in some cases,
    /// so an empty type counts as author.
    fn is_author(&self) -> bool {
        self.creator_type.is_empty() || self.creator_type == "author"
    }
}

/// Extract the `testCases` JSON array from a translator file (between the
/// `/** BEGIN TEST CASES **/` … `/** END TEST CASES **/` markers) and return the
/// `import`-type cases.
fn import_cases(translator_file: &str) -> Vec<TestCase> {
    let path = translators_dir().join(translator_file);
    let src =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    let begin = src
        .find("var testCases")
        .and_then(|i| src[i..].find('[').map(|j| i + j))
        .expect("testCases array start");
    // The array ends at the top-level closing bracket before END TEST CASES.
    let end_marker = src.find("/** END TEST CASES **/").unwrap_or(src.len());
    let close = src[..end_marker].rfind(']').expect("testCases array end");
    let json = &src[begin..=close];

    let cases: Vec<TestCase> = serde_json::from_str(json)
        .unwrap_or_else(|e| panic!("parse {translator_file} testCases: {e}"));
    cases.into_iter().filter(|c| c.kind == "import").collect()
}

/// Normalize a string for comparison: trim, collapse internal whitespace.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the 4-digit year from a Zotero `date` string (which may be
/// "2011", "June 22, 2011", "2011-06", …).
fn year_of(date: &str) -> Option<i32> {
    date.split(|c: char| !c.is_ascii_digit())
        .find(|t| t.len() == 4)
        .and_then(|t| t.parse().ok())
}

/// A single field mismatch between expected and parsed.
struct Mismatch {
    field: &'static str,
    expected: String,
    got: String,
}

/// Compare one parsed [`Paper`] against a Zotero expected item over the modeled
/// fields, returning any mismatches.
fn compare(expected: &ExpectedItem, got: &Paper) -> Vec<Mismatch> {
    let mut out = Vec::new();
    let mut check = |field: &'static str, exp: &str, got: &str| {
        if !exp.is_empty() && norm(exp) != norm(got) {
            out.push(Mismatch {
                field,
                expected: norm(exp),
                got: norm(got),
            });
        }
    };

    check("title", &expected.title, &got.title);
    check("DOI", &expected.doi, got.doi.as_deref().unwrap_or(""));
    check(
        "publicationTitle",
        &expected.publication_title,
        got.publication.journal.as_deref().unwrap_or(""),
    );
    check(
        "volume",
        &expected.volume,
        got.publication.volume.as_deref().unwrap_or(""),
    );
    check(
        "issue",
        &expected.issue,
        got.publication.issue.as_deref().unwrap_or(""),
    );
    check(
        "pages",
        &expected.pages,
        got.publication.pages.as_deref().unwrap_or(""),
    );
    check(
        "publisher",
        &expected.publisher,
        got.publication.publisher.as_deref().unwrap_or(""),
    );

    // Year (from the expected `date`). Skip degenerate synthetic years (<= 0):
    // Zotero's "every field" torture cases use a placeholder "0000" date that a
    // real paper never has and the flat model can't represent.
    if let Some(exp_year) = year_of(&expected.date)
        && exp_year > 0
        && got.year != Some(exp_year)
    {
        out.push(Mismatch {
            field: "year",
            expected: exp_year.to_string(),
            got: got.year.map(|y| y.to_string()).unwrap_or_default(),
        });
    }

    // Authors: compare the ordered list of display names. The flat `Paper` models
    // only authors, so non-author creator roles (editors, translators, producers)
    // are out of scope and excluded from the expected list.
    let exp_authors: Vec<String> = expected
        .creators
        .iter()
        .filter(|c| c.is_author() && !c.display().is_empty())
        .map(|c| norm(&c.display()))
        .collect();
    if !exp_authors.is_empty() {
        let got_authors: Vec<String> = got.author_names().iter().map(|a| norm(a)).collect();
        if exp_authors != got_authors {
            out.push(Mismatch {
                field: "creators",
                expected: exp_authors.join(" | "),
                got: got_authors.join(" | "),
            });
        }
    }

    out
}

/// Outcome of running one translator's import cases.
struct Coverage {
    total: usize,
    passed: usize,
    /// Per-case diagnostics for the cases that didn't fully match.
    failures: Vec<String>,
}

/// Run every `import` test case in `translator_file` through `import_fn` and diff
/// each against its expected item. A case "passes" when every modeled field of
/// every item matches. Returns the pass count plus per-case diagnostics — this is
/// a *coverage* measurement, not an all-or-nothing gate, because Zotero's items
/// carry creator types and edge cases our flat `Paper` doesn't model.
fn run_format(
    translator_file: &str,
    import_fn: impl Fn(&str) -> Result<Vec<Paper>, String>,
) -> Coverage {
    let cases = import_cases(translator_file);
    assert!(
        !cases.is_empty(),
        "{translator_file} has no import test cases — extractor broke?"
    );

    let mut passed = 0;
    let mut failures: Vec<String> = Vec::new();

    for (i, case) in cases.iter().enumerate() {
        let expected: Vec<ExpectedItem> = match serde_json::from_value(case.items.clone()) {
            Ok(items) => items,
            Err(e) => {
                failures.push(format!("case {i}: expected items failed to parse: {e}"));
                continue;
            }
        };

        let parsed = match import_fn(&case.input) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("case {i}: import failed: {e}"));
                continue;
            }
        };

        if parsed.len() != expected.len() {
            failures.push(format!(
                "case {i}: item count {} != expected {}",
                parsed.len(),
                expected.len()
            ));
            continue;
        }

        let mut case_msgs = Vec::new();
        for (j, (exp, got)) in expected.iter().zip(parsed.iter()).enumerate() {
            for m in compare(exp, got) {
                case_msgs.push(format!(
                    "  case {i} item {j} [{}]: expected {:?}, got {:?}",
                    m.field, m.expected, m.got
                ));
            }
        }
        if case_msgs.is_empty() {
            passed += 1;
        } else {
            failures.extend(case_msgs);
        }
    }

    Coverage {
        total: cases.len(),
        passed,
        failures,
    }
}

/// Assert a translator's import coverage meets `floor` passing cases, printing
/// the pass rate and (on a shortfall) the diagnostics. The floor is a ratchet:
/// raise it as the importers improve; a regression that drops below it fails.
fn assert_coverage(
    translator_file: &str,
    floor: usize,
    import_fn: impl Fn(&str) -> Result<Vec<Paper>, String>,
) {
    let cov = run_format(translator_file, import_fn);
    println!(
        "{translator_file}: {}/{} import cases pass ({}%), floor {floor}",
        cov.passed,
        cov.total,
        cov.passed * 100 / cov.total.max(1),
    );
    // Always print the remaining mismatches (visible under `--nocapture`) so the
    // real bugs are readable without having to force an assertion failure.
    for f in &cov.failures {
        println!("{f}");
    }
    assert!(
        cov.passed >= floor,
        "{translator_file} coverage regressed: {}/{} passed, need >= {floor}.",
        cov.passed,
        cov.total,
    );
}

// Coverage floors are ratchets pinned to the current passing counts over the
// fields our flat `Paper` models. The remaining shortfall is model scope (Zotero
// items carry creator roles and fields the flat `Paper` doesn't store) plus a few
// backend-level edge cases (e.g. `biblatex` collapsing distinct TeX escapes, or
// rejecting inputs Zotero's line-tolerant parser accepts). Raise a floor whenever
// an importer fix lifts its pass count; a drop below the floor is a regression.

#[test]
fn ris_import_coverage() {
    assert_coverage("RIS.js", 11, rotero_bib::import_ris);
}

#[test]
fn nbib_import_coverage() {
    assert_coverage("MEDLINEnbib.js", 9, rotero_bib::import_nbib);
}

#[test]
fn csl_json_import_coverage() {
    assert_coverage("CSL JSON.js", 1, rotero_bib::import_csl_json);
}

#[test]
fn bibtex_import_coverage() {
    assert_coverage("BibTeX.js", 17, |input| {
        rotero_bib::import_bibtex(input)
            .map(|entries| entries.into_iter().map(|e| e.paper).collect())
    });
}

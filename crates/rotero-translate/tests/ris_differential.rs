//! Differential test: our Rust RIS parser vs Zotero's own `RIS.js`, on identical
//! input.
//!
//! For each RIS `import` test case bundled in `RIS.js`, the same input string is
//! run through both [`rotero_bib::import_ris`] and the vendored `RIS.js`
//! (executed via [`run_import_translator`]). The two outputs are diffed
//! field-by-field over what the flat `Paper` models. Unlike `import_testcases.rs`
//! — which compares against Zotero's *hand-maintained* expected `items` — this
//! compares against Zotero's *actual parsing logic* on the same bytes, so it
//! surfaces divergences the bundled expectations happen to hide.
//!
//! Requires the boa `main` pin (0.21.1 can't construct `RIS.js`'s singletons;
//! see the engine `Cargo.toml`). Gated on `translator-engine`.
#![cfg(feature = "translator-engine")]

use std::path::PathBuf;

use rotero_models::Paper;
use rotero_translate::ZoteroItem;
use rotero_translate::engine::run_import_translator;

const RIS_JS: &str = include_str!("../vendor/translators/RIS.js");

/// Strip a vendored translator's JSON metadata header, leaving the JS body.
fn translator_body(full: &str) -> &str {
    let start = full.find('{').expect("translator header");
    let bytes = full.as_bytes();
    let (mut depth, mut in_str, mut esc) = (0i32, false, false);
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_str {
            match c {
                b'\\' if !esc => esc = true,
                b'"' if !esc => in_str = false,
                _ => esc = false,
            }
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return &full[i + 1..];
                }
            }
            _ => {}
        }
    }
    panic!("unterminated translator header");
}

/// The `import` test-case inputs bundled in `RIS.js`.
fn ris_inputs() -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct Case {
        #[serde(rename = "type")]
        kind: String,
        #[serde(default)]
        input: String,
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/translators/RIS.js");
    let src = std::fs::read_to_string(&path).expect("read RIS.js");
    let begin = src
        .find("var testCases")
        .and_then(|i| src[i..].find('[').map(|j| i + j))
        .expect("testCases start");
    let end = src.find("/** END TEST CASES **/").unwrap_or(src.len());
    let close = src[..end].rfind(']').expect("testCases end");
    let cases: Vec<Case> = serde_json::from_str(&src[begin..=close]).expect("parse testCases");
    cases
        .into_iter()
        .filter(|c| c.kind == "import" && !c.input.is_empty())
        .map(|c| c.input)
        .collect()
}

/// Collapse whitespace for comparison.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The Zotero item's authors as "First Last" strings.
fn js_authors(item: &ZoteroItem) -> Vec<String> {
    item.creators
        .iter()
        .filter(|c| c.creator_type.is_empty() || c.creator_type == "author")
        .map(|c| {
            if !c.name.is_empty() {
                norm(&c.name)
            } else if c.first_name.is_empty() {
                norm(&c.last_name)
            } else {
                norm(&format!("{} {}", c.first_name, c.last_name))
            }
        })
        .collect()
}

/// Field-level divergences between our `Paper` and Zotero's `ZoteroItem`.
fn diff(ours: &Paper, theirs: &ZoteroItem) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |field: &str, a: &str, b: &str| {
        if norm(a) != norm(b) {
            out.push(format!("{field}: ours={:?} zotero={:?}", norm(a), norm(b)));
        }
    };
    check("title", &ours.title, &theirs.title);
    check("DOI", ours.doi.as_deref().unwrap_or(""), &theirs.doi);
    check(
        "publicationTitle",
        ours.publication.journal.as_deref().unwrap_or(""),
        &theirs.publication_title,
    );
    check(
        "volume",
        ours.publication.volume.as_deref().unwrap_or(""),
        &theirs.volume,
    );
    check(
        "issue",
        ours.publication.issue.as_deref().unwrap_or(""),
        &theirs.issue,
    );
    check(
        "pages",
        ours.publication.pages.as_deref().unwrap_or(""),
        &theirs.pages,
    );
    check(
        "publisher",
        ours.publication.publisher.as_deref().unwrap_or(""),
        &theirs.publisher,
    );

    let our_authors: Vec<String> = ours.author_names().iter().map(|a| norm(a)).collect();
    let their_authors = js_authors(theirs);
    if our_authors != their_authors {
        out.push(format!(
            "creators: ours={:?} zotero={:?}",
            our_authors.join(" | "),
            their_authors.join(" | ")
        ));
    }
    out
}

/// For each bundled RIS input, run both parsers and report cases where our output
/// diverges from Zotero's `RIS.js` over the modeled fields. Prints the
/// divergences (visible under `--nocapture`) and asserts an agreement floor so a
/// regression that widens the gap fails.
#[test]
fn ris_agrees_with_zotero_js() {
    let body = translator_body(RIS_JS);
    let inputs = ris_inputs();
    assert!(!inputs.is_empty(), "no RIS import inputs extracted");

    let mut agree = 0;
    let mut notes = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        let ours = rotero_bib::import_ris(input).unwrap_or_default();
        let theirs = run_import_translator(body, input).unwrap_or_default();

        if ours.len() != theirs.len() {
            notes.push(format!(
                "case {i}: item count ours={} zotero={}",
                ours.len(),
                theirs.len()
            ));
            continue;
        }
        let mut case_notes = Vec::new();
        for (j, (o, t)) in ours.iter().zip(theirs.iter()).enumerate() {
            for d in diff(o, t) {
                case_notes.push(format!("  case {i} item {j} [{d}]"));
            }
        }
        if case_notes.is_empty() {
            agree += 1;
        } else {
            notes.extend(case_notes);
        }
    }

    println!(
        "RIS Rust vs RIS.js: {agree}/{} cases agree on all modeled fields",
        inputs.len()
    );
    for n in &notes {
        println!("{n}");
    }

    // Agreement floor (ratchet). Every real single-item case now agrees; the
    // remaining disagreements are the two synthetic "every RIS tag across every
    // item type" torture cases, which route tags to Zotero fields the flat
    // `Paper` doesn't model (a `case`'s reporter volume, an audio recording's
    // album title, …), plus one input the boa engine drops but our parser keeps.
    // Raise the floor as the Rust parser converges further; a drop below it is a
    // regression.
    const FLOOR: usize = 9;
    assert!(
        agree >= FLOOR,
        "RIS/JS agreement regressed: {agree}/{} agree, need >= {FLOOR}",
        inputs.len()
    );
}

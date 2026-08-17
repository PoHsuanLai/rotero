//! Link extraction — intra-document jumps and external URIs.

use rotero_pdf::{LinkTarget, PdfEngine};

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/pdfs/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// Bind PDFium, or `None` when the dynamic library isn't on the search path.
///
/// PDFium is a native dependency fetched by `just setup-pdfium` into `lib/`; a
/// bare `cargo test` (e.g. in CI, which doesn't download it) has no library to
/// bind. Skip rather than fail there — the assertions below only exercise our
/// extraction logic, not PDFium itself.
fn engine_or_skip() -> Option<PdfEngine> {
    match PdfEngine::new(None) {
        Ok(engine) => Some(engine),
        // Where PDFium was deliberately provisioned, failing to bind is a
        // failure rather than a skip: without this these tests quietly return
        // to passing by doing nothing, which is how they spent their whole
        // existence before CI began installing the library.
        //
        // Keyed on `PDFIUM_DYNAMIC_LIB_PATH` rather than `CI`, because not every
        // CI job provisions it — the Linux and Windows jobs do not, and
        // demanding it there would fail over a library never meant to be there.
        //
        // Worth being honest about the reach: the resolver also probes next to
        // the executable, so a *wrong* path still binds and this does not fire.
        // What it does catch is PDFium being absent altogether on a job that is
        // supposed to have it, which is the regression that matters.
        Err(e) if std::env::var("PDFIUM_DYNAMIC_LIB_PATH").is_ok_and(|p| !p.is_empty()) => {
            panic!("PDFium was provisioned but could not be bound: {e}");
        }
        Err(e) => {
            eprintln!("skipping: PDFium unavailable ({e})");
            None
        }
    }
}

#[test]
fn extracts_internal_links_with_resolved_targets() {
    let Some(mut engine) = engine_or_skip() else {
        return;
    };
    let links = engine
        .extract_links(&fixture("basicapi.pdf"))
        .expect("extract links");

    assert!(!links.is_empty(), "expected internal links, got none");
    for l in &links {
        // Source rect belongs to a real, positively-sized page.
        assert!(l.page_width_pts > 0.0 && l.page_height_pts > 0.0);
        match &l.target {
            LinkTarget::Internal { y_frac, .. } => {
                if let Some(f) = y_frac {
                    assert!((0.0..=1.0).contains(f));
                }
            }
            LinkTarget::External { uri } => assert!(!uri.is_empty()),
        }
    }
}

#[test]
fn extracts_external_uri_links() {
    let Some(mut engine) = engine_or_skip() else {
        return;
    };
    let links = engine
        .extract_links(&fixture("basicapi.pdf"))
        .expect("extract links");

    let external: Vec<&str> = links
        .iter()
        .filter_map(|l| match &l.target {
            LinkTarget::External { uri } => Some(uri.as_str()),
            LinkTarget::Internal { .. } => None,
        })
        .collect();
    assert!(
        !external.is_empty(),
        "expected at least one external URI link"
    );
}

#[test]
fn link_free_pdf_yields_no_links() {
    let Some(mut engine) = engine_or_skip() else {
        return;
    };
    // tracemonkey.pdf carries no link annotations — extraction must be empty,
    // not error.
    let links = engine
        .extract_links(&fixture("tracemonkey.pdf"))
        .expect("extract links");
    assert!(links.is_empty());
}

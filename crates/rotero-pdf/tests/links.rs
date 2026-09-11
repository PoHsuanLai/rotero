//! Link extraction — intra-document jumps and external URIs.

use rotero_pdf::{LinkTarget, PdfEngine};

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/pdfs/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn engine() -> PdfEngine {
    PdfEngine::new()
}

#[test]
fn extracts_internal_links_with_resolved_targets() {
    let engine = engine();
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
    let engine = engine();
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
    let engine = engine();
    // tracemonkey.pdf carries no link annotations — extraction must be empty,
    // not error.
    let links = engine
        .extract_links(&fixture("tracemonkey.pdf"))
        .expect("extract links");
    assert!(links.is_empty());
}

//! Link extraction — intra-document jumps and external URIs.

use rotero_pdf::{DocCache, LinkTarget, extract_links};

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/pdfs/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn extracts_internal_links_with_resolved_targets() {
    let cache = DocCache::new();
    let doc = cache.open(&fixture("basicapi.pdf")).expect("open");
    let links = extract_links(&doc).expect("extract links");

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
    let cache = DocCache::new();
    let doc = cache.open(&fixture("basicapi.pdf")).expect("open");
    let links = extract_links(&doc).expect("extract links");

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
fn link_annot_free_pdf_may_still_surface_web_links() {
    let cache = DocCache::new();
    // tracemonkey.pdf carries no /Link annotations, but body text may still
    // contain bare URLs/DOIs that `TextPage::web_links` surfaces.
    let doc = cache.open(&fixture("tracemonkey.pdf")).expect("open");
    let links = extract_links(&doc).expect("extract links");
    for l in &links {
        match &l.target {
            LinkTarget::External { uri } => assert!(!uri.is_empty()),
            LinkTarget::Internal { .. } => {
                panic!("tracemonkey should not have internal /Link targets")
            }
        }
    }
}

#[test]
fn web_links_merge_does_not_double_count_identical_uri_rect() {
    let cache = DocCache::new();
    let doc = cache.open(&fixture("basicapi.pdf")).expect("open");
    let links = extract_links(&doc).expect("extract links");
    // Group external URI+rounded-rect; each key should appear once.
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for l in &links {
        let LinkTarget::External { uri } = &l.target else {
            continue;
        };
        let key = (
            l.page,
            uri.to_ascii_lowercase(),
            (l.rect_pts[0] * 10.0).round() as i32,
            (l.rect_pts[1] * 10.0).round() as i32,
            (l.rect_pts[2] * 10.0).round() as i32,
            (l.rect_pts[3] * 10.0).round() as i32,
        );
        assert!(
            seen.insert(key),
            "duplicate external link: {uri:?} on page {}",
            l.page
        );
    }
}

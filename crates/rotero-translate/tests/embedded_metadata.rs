//! Golden tests for the Embedded Metadata extraction (`extract_zotero_item`).
//! Deterministic and offline — assert field-by-field on representative pages.

use rotero_translate::extract_zotero_item;

const CITATION_RICH: &str = r#"
<!DOCTYPE html>
<html lang="en">
<head>
  <title>Publisher — Attention Is All You Need</title>
  <meta name="citation_title" content="Attention Is All You Need">
  <meta name="citation_author" content="Vaswani, Ashish">
  <meta name="citation_author" content="Shazeer, Noam">
  <meta name="citation_doi" content="10.5555/3295222.3295349">
  <meta name="citation_journal_title" content="Advances in Neural Information Processing Systems">
  <meta name="citation_publication_date" content="2017/12/04">
  <meta name="citation_volume" content="30">
  <meta name="citation_issue" content="2">
  <meta name="citation_firstpage" content="5998">
  <meta name="citation_lastpage" content="6008">
  <meta name="citation_publisher" content="Curran Associates">
  <meta name="citation_issn" content="1049-5258">
  <meta name="citation_pdf_url" content="https://example.org/paper.pdf">
  <meta name="citation_abstract" content="The dominant sequence transduction models...">
  <meta name="citation_keywords" content="attention, transformers, sequence modeling">
</head>
<body></body>
</html>
"#;

const DC_ONLY: &str = r#"
<!DOCTYPE html>
<html lang="fr">
<head>
  <title>Ignored title tag</title>
  <meta name="DC.title" content="Une Étude sur les Réseaux">
  <meta name="DC.creator" content="Marie Curie">
  <meta name="DC.identifier" content="10.1234/example.dc">
  <meta name="DC.date" content="2019-05-01">
  <meta name="DC.publisher" content="Presses Universitaires">
</head>
<body></body>
</html>
"#;

const JSONLD_ONLY: &str = r#"
<!DOCTYPE html>
<html>
<head>
  <title>Some Journal</title>
  <script type="application/ld+json">
  {
    "@context": "https://schema.org",
    "@type": "ScholarlyArticle",
    "name": "Deep Residual Learning",
    "author": [
      {"@type": "Person", "givenName": "Kaiming", "familyName": "He"},
      {"@type": "Person", "name": "Xiangyu Zhang"}
    ],
    "datePublished": "2016-06-27",
    "isPartOf": {"@type": "Periodical", "name": "CVPR"},
    "description": "We present a residual learning framework."
  }
  </script>
</head>
<body></body>
</html>
"#;

#[test]
fn citation_rich_extracts_all_fields() {
    let item = extract_zotero_item(CITATION_RICH);
    assert_eq!(item.title, "Attention Is All You Need");
    assert_eq!(item.item_type, "journalArticle");
    assert_eq!(item.doi, "10.5555/3295222.3295349");
    assert_eq!(
        item.publication_title,
        "Advances in Neural Information Processing Systems"
    );
    assert_eq!(item.volume, "30");
    assert_eq!(item.issue, "2");
    assert_eq!(item.pages, "5998-6008");
    assert_eq!(item.publisher, "Curran Associates");
    assert_eq!(item.issn, "1049-5258");
    assert_eq!(item.language, "en"); // from <html lang>

    // structured creators (Last, First form)
    assert_eq!(item.creators.len(), 2);
    assert_eq!(item.creators[0].last_name, "Vaswani");
    assert_eq!(item.creators[0].first_name, "Ashish");
    assert_eq!(item.creators[0].creator_type, "author");

    // PDF becomes an attachment
    let pdf = item
        .attachments
        .iter()
        .find(|a| a.mime_type == "application/pdf")
        .expect("pdf attachment");
    assert_eq!(pdf.url, "https://example.org/paper.pdf");

    // keywords split into tags
    let tags: Vec<&str> = item.tags.iter().map(|t| t.tag.as_str()).collect();
    assert!(tags.contains(&"attention"));
    assert!(tags.contains(&"transformers"));
    assert!(tags.contains(&"sequence modeling"));

    // date preserved as string; year derived
    assert!(item.date.starts_with("2017"));
}

#[test]
fn dc_only_page_extracts_dublin_core() {
    let item = extract_zotero_item(DC_ONLY);
    assert_eq!(item.title, "Une Étude sur les Réseaux");
    assert_eq!(item.doi, "10.1234/example.dc"); // DC.identifier starting with 10.
    assert_eq!(item.publisher, "Presses Universitaires");
    assert_eq!(item.language, "fr"); // <html lang>
    assert_eq!(item.creators.len(), 1);
    // "Marie Curie" — space form splits to first/last
    assert_eq!(item.creators[0].first_name, "Marie");
    assert_eq!(item.creators[0].last_name, "Curie");
}

#[test]
fn jsonld_only_page_extracts_scholarly_article() {
    let item = extract_zotero_item(JSONLD_ONLY);
    assert_eq!(item.title, "Deep Residual Learning");
    assert_eq!(item.publication_title, "CVPR");
    assert_eq!(item.abstract_note, "We present a residual learning framework.");
    assert_eq!(item.creators.len(), 2);
    // structured (givenName/familyName) and name-only both handled
    assert_eq!(item.creators[0].last_name, "He");
    assert_eq!(item.creators[0].first_name, "Kaiming");
    assert_eq!(item.creators[1].last_name, "Zhang");
}

#[test]
fn title_falls_back_to_title_tag() {
    let html = r#"<html><head><title>Fallback Title</title></head><body></body></html>"#;
    let item = extract_zotero_item(html);
    assert_eq!(item.title, "Fallback Title");
}

#[test]
fn empty_page_yields_empty_title() {
    let item = extract_zotero_item("<html><head></head><body></body></html>");
    assert_eq!(item.title, "");
}

#[test]
fn authors_are_not_doubled_across_vocabularies() {
    // A page that lists the same authors under both citation_author and
    // dc.creator must not count them twice (as e.g. Nature pages do).
    let html = r#"<html><head>
        <meta name="citation_title" content="Two Vocabularies">
        <meta name="citation_author" content="Kucsko, G.">
        <meta name="citation_author" content="Maurer, P. C.">
        <meta name="dc.creator" content="Kucsko, G.">
        <meta name="dc.creator" content="Maurer, P. C.">
    </head><body></body></html>"#;
    let item = extract_zotero_item(html);
    assert_eq!(item.creators.len(), 2, "authors should come from one vocabulary, not both");
    assert_eq!(item.creators[0].last_name, "Kucsko");
    assert_eq!(item.creators[1].last_name, "Maurer");
}

#[test]
fn firstpage_only_yields_single_page() {
    // A page with citation_firstpage but no citation_lastpage should still yield
    // pages set to the first page (Zotero/Node behavior), not drop it.
    let html = r#"<html><head>
        <meta name="citation_title" content="First Page Only">
        <meta name="citation_firstpage" content="37">
    </head><body></body></html>"#;
    let item = extract_zotero_item(html);
    assert_eq!(item.pages, "37");
}

#[test]
fn body_abstract_replaces_truncated_meta() {
    // When the only abstract meta is a truncated description snippet, the fuller
    // abstract rendered in the page body should win.
    let html = r#"<html><head>
        <meta name="citation_title" content="Body Abstract">
        <meta name="description" content="Short truncated snippet of the abstract text goes here and then ...">
    </head><body>
        <section class="abstract" id="abstract1"><h2>Abstract</h2>
        <p>This is the complete abstract paragraph rendered in the page body. It is substantially longer than the truncated meta snippet and contains the full text a reader would want.</p>
        </section>
    </body></html>"#;
    let item = extract_zotero_item(html);
    assert!(
        item.abstract_note.starts_with("This is the complete abstract"),
        "body abstract should win over truncated meta, got: {:?}",
        item.abstract_note
    );
    // The bare "Abstract" heading is dropped, not included as content.
    assert!(!item.abstract_note.to_lowercase().starts_with("abstract"));
}

#[test]
fn body_abstract_used_when_no_meta_abstract() {
    let html = r#"<html><head>
        <meta name="citation_title" content="No Meta Abstract">
    </head><body>
        <div class="abstract"><p>The abstract lives only in the body of this document and there is no meta tag carrying it at all.</p></div>
    </body></html>"#;
    let item = extract_zotero_item(html);
    assert!(item.abstract_note.starts_with("The abstract lives only in the body"));
}

#[test]
fn full_meta_abstract_not_overridden_by_body() {
    // A complete (non-truncated) meta abstract must be preserved even if a body
    // abstract element is also present — meta is authoritative when complete.
    let html = r#"<html><head>
        <meta name="citation_title" content="Full Meta Abstract">
        <meta name="citation_abstract" content="Complete abstract from meta tag.">
    </head><body>
        <section class="abstract"><p>A different, longer body abstract that should be ignored because the meta abstract is already complete.</p></section>
    </body></html>"#;
    let item = extract_zotero_item(html);
    assert_eq!(item.abstract_note, "Complete abstract from meta tag.");
}

#[test]
fn pmc_fixture_hub_yields_pages_and_body_abstract() {
    // Regression for the PMC gap: the article HTML has citation_firstpage but no
    // citation_lastpage, and renders the full abstract in a <section
    // class="abstract"> (the meta description is only a truncated snippet).
    let item = extract_zotero_item(include_str!("fixtures/pmc_PMC2377243.html"));
    assert_eq!(item.pages, "37", "firstpage-only should yield pages=37");
    assert!(
        item.abstract_note.contains("long-term oxygen therapy")
            && item.abstract_note.contains("guinea pig"),
        "body abstract should be extracted in full"
    );
    assert!(
        item.abstract_note.len() > 1000,
        "expected the full body abstract, not the truncated meta snippet; got {} chars",
        item.abstract_note.len()
    );
    // Sanity: the truncated meta snippet ended in an ellipsis; the body version
    // must not.
    assert!(!item.abstract_note.trim_end().ends_with("..."));
}

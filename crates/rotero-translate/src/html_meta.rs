//! HTML metadata extraction: reads scholarly metadata from `<meta>` tags
//! (Highwire `citation_*`, Dublin Core, PRISM, eprints, OpenGraph) and JSON-LD.
//!
//! This is the shared "Embedded Metadata"-style core. It lives here (rather than
//! in `rotero-connector`) so both the connector's fetch-then-scrape path and the
//! native translator registry can reuse it without a circular dependency.

use rotero_models::Paper;
use scraper::{Html, Selector};
use serde::Deserialize;

use crate::item::{ZoteroCreator, ZoteroItem, ZoteroTag};

#[derive(Debug, Default)]
struct ScrapedFields {
    title: Option<String>,
    authors: Vec<String>,
    doi: Option<String>,
    isbn: Option<String>,
    issn: Option<String>,
    url: Option<String>,
    pdf_url: Option<String>,
    journal: Option<String>,
    year: Option<i32>,
    date: Option<String>,
    volume: Option<String>,
    issue: Option<String>,
    pages: Option<String>,
    publisher: Option<String>,
    abstract_text: Option<String>,
    language: Option<String>,
    keywords: Vec<String>,
    item_type: Option<String>,
}

impl ScrapedFields {
    fn into_zotero_item(self) -> ZoteroItem {
        let creators = self
            .authors
            .into_iter()
            .map(|a| split_creator(&a))
            .collect();
        let tags = self
            .keywords
            .into_iter()
            .map(|k| ZoteroTag {
                tag: k,
                tag_type: 1,
            })
            .collect();
        ZoteroItem {
            item_type: self.item_type.unwrap_or_else(|| "journalArticle".to_string()),
            title: self.title.unwrap_or_default(),
            creators,
            date: self.date.unwrap_or_default(),
            url: self.url.unwrap_or_default(),
            doi: self.doi.unwrap_or_default(),
            isbn: self.isbn.unwrap_or_default(),
            issn: self.issn.unwrap_or_default(),
            abstract_note: self.abstract_text.unwrap_or_default(),
            publication_title: self.journal.unwrap_or_default(),
            volume: self.volume.unwrap_or_default(),
            issue: self.issue.unwrap_or_default(),
            pages: self.pages.unwrap_or_default(),
            publisher: self.publisher.unwrap_or_default(),
            language: self.language.unwrap_or_default(),
            tags,
            attachments: pdf_attachment(self.pdf_url),
            ..Default::default()
        }
    }
}

/// Build a PDF attachment list from an optional URL (empty if none).
fn pdf_attachment(pdf_url: Option<String>) -> Vec<crate::item::ZoteroAttachment> {
    match pdf_url {
        Some(u) if !u.is_empty() => vec![crate::item::ZoteroAttachment {
            title: "Full Text PDF".to_string(),
            url: u,
            mime_type: "application/pdf".to_string(),
            ..Default::default()
        }],
        _ => Vec::new(),
    }
}

/// Split an author string into a structured creator. Handles "Last, First"
/// (comma form) and "First Last" (space form); falls back to a name-only
/// creator when the shape is ambiguous.
fn split_creator(author: &str) -> ZoteroCreator {
    let author = author.trim();
    let make = |first: &str, last: &str| ZoteroCreator {
        first_name: first.trim().to_string(),
        last_name: last.trim().to_string(),
        name: String::new(),
        creator_type: "author".to_string(),
    };
    if let Some((last, first)) = author.split_once(',') {
        return make(first, last);
    }
    if let Some(idx) = author.rfind(' ') {
        return make(&author[..idx], &author[idx + 1..]);
    }
    ZoteroCreator {
        first_name: String::new(),
        last_name: String::new(),
        name: author.to_string(),
        creator_type: "author".to_string(),
    }
}

/// Extracts scholarly metadata from HTML meta tags and JSON-LD into a [`Paper`].
pub fn extract_from_html(html: &str) -> Paper {
    extract_zotero_item(html)
        .into_paper()
        .unwrap_or_else(|| Paper {
            title: "Untitled".to_string(),
            ..Default::default()
        })
}

/// Extracts scholarly metadata from HTML meta tags and JSON-LD into the richer
/// [`ZoteroItem`] (captures ISSN, language, keywords, structured creators, and
/// item type — fields the flat `Paper` model drops).
#[allow(clippy::field_reassign_with_default)] // fields computed incrementally from HTML
pub fn extract_zotero_item(html: &str) -> ZoteroItem {
    let doc = Html::parse_document(html);
    let mut meta = ScrapedFields::default();

    meta.doi = get_meta(
        &doc,
        &[
            ("name", "citation_doi"),
            ("property", "citation_doi"),
            ("name", "prism.doi"),
        ],
    );
    if meta.doi.is_none() {
        // DC.identifier might contain a DOI
        if let Some(dc) = get_meta(
            &doc,
            &[("name", "DC.identifier"), ("name", "dc.identifier")],
        ) && dc.starts_with("10.")
        {
            meta.doi = Some(dc);
        }
    }
    if let Some(ref mut doi) = meta.doi {
        for prefix in ["https://doi.org/", "http://doi.org/"] {
            if let Some(stripped) = doi.strip_prefix(prefix) {
                *doi = stripped.to_string();
                break;
            }
        }
    }

    meta.title = get_meta(
        &doc,
        &[
            ("name", "citation_title"),
            ("name", "DC.title"),
            ("name", "dc.title"),
            ("name", "eprints.title"),
            ("property", "og:title"),
        ],
    );

    meta.authors = get_all_meta(
        &doc,
        &[
            ("name", "citation_author"),
            ("name", "bepress_citation_author"),
            ("name", "DC.creator"),
            ("name", "dc.creator"),
            ("name", "eprints.creators_name"),
        ],
    );

    meta.pdf_url = get_meta(
        &doc,
        &[
            ("name", "citation_pdf_url"),
            ("name", "bepress_citation_pdf_url"),
        ],
    );

    meta.journal = get_meta(
        &doc,
        &[
            ("name", "citation_journal_title"),
            ("name", "bepress_citation_journal_title"),
            ("name", "prism.publicationName"),
            ("name", "DC.source"),
            ("name", "dc.source"),
        ],
    );

    meta.date = get_meta(
        &doc,
        &[
            ("name", "citation_publication_date"),
            ("name", "citation_date"),
            ("name", "bepress_citation_date"),
            ("name", "DC.date"),
            ("name", "dc.date"),
            ("property", "article:published_time"),
        ],
    );
    if let Some(ref d) = meta.date
        && let Some(year) = extract_year(d)
    {
        meta.year = Some(year);
    }

    meta.volume = get_meta(&doc, &[("name", "citation_volume")]);
    meta.issue = get_meta(&doc, &[("name", "citation_issue")]);
    let first_page = get_meta(&doc, &[("name", "citation_firstpage")]);
    let last_page = get_meta(&doc, &[("name", "citation_lastpage")]);
    if let Some(fp) = first_page {
        meta.pages = Some(match last_page {
            Some(lp) => format!("{fp}-{lp}"),
            None => fp,
        });
    }

    meta.publisher = get_meta(
        &doc,
        &[
            ("name", "citation_publisher"),
            ("name", "DC.publisher"),
            ("name", "dc.publisher"),
        ],
    );

    meta.abstract_text = get_meta(
        &doc,
        &[
            ("name", "citation_abstract"),
            ("name", "DC.description"),
            ("name", "dc.description"),
            ("name", "description"),
            ("property", "og:description"),
        ],
    );

    meta.isbn = get_meta(&doc, &[("name", "citation_isbn")]);
    meta.issn = get_meta(
        &doc,
        &[
            ("name", "citation_issn"),
            ("name", "prism.issn"),
            ("name", "prism.eIssn"),
        ],
    );

    meta.language = get_meta(
        &doc,
        &[
            ("name", "citation_language"),
            ("name", "DC.language"),
            ("name", "dc.language"),
        ],
    )
    .or_else(|| html_lang(&doc));

    meta.keywords = get_all_meta(
        &doc,
        &[
            ("name", "citation_keywords"),
            ("name", "DC.subject"),
            ("name", "dc.subject"),
            ("name", "keywords"),
        ],
    )
    .into_iter()
    // citation_keywords / keywords are often a single comma/semicolon list.
    .flat_map(|k| {
        k.split([',', ';'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    })
    .collect();

    extract_jsonld(&doc, &mut meta);

    // Title fallback: <title>, then first <h1>, before giving up.
    if meta.title.is_none() {
        meta.title = doc_title(&doc).or_else(|| first_h1(&doc));
    }

    meta.item_type = Some(infer_item_type(&doc, &meta));

    meta.into_zotero_item()
}

/// Infer a Zotero item type from the metadata present. Defaults to
/// `journalArticle` (the overwhelming common case for scholarly pages).
fn infer_item_type(doc: &Html, meta: &ScrapedFields) -> String {
    if get_meta(doc, &[("name", "citation_conference_title")]).is_some()
        || get_meta(doc, &[("name", "citation_conference")]).is_some()
    {
        return "conferencePaper".to_string();
    }
    if get_meta(doc, &[("name", "citation_dissertation_institution")]).is_some()
        || get_meta(doc, &[("name", "citation_dissertation_name")]).is_some()
    {
        return "thesis".to_string();
    }
    if get_meta(doc, &[("name", "citation_inbook_title")]).is_some() {
        return "bookSection".to_string();
    }
    if meta.isbn.is_some()
        && meta.journal.is_none()
        && get_meta(doc, &[("name", "citation_title")]).is_some()
    {
        return "book".to_string();
    }
    "journalArticle".to_string()
}

/// The `<html lang="…">` attribute, if present.
fn html_lang(doc: &Html) -> Option<String> {
    let sel = Selector::parse("html").ok()?;
    let el = doc.select(&sel).next()?;
    el.value()
        .attr("lang")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// The `<title>` text, if present and non-empty.
fn doc_title(doc: &Html) -> Option<String> {
    let sel = Selector::parse("title").ok()?;
    let text = doc.select(&sel).next()?.text().collect::<String>();
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// The first `<h1>` text, if present and non-empty.
fn first_h1(doc: &Html) -> Option<String> {
    let sel = Selector::parse("h1").ok()?;
    let text = doc.select(&sel).next()?.text().collect::<String>();
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn get_meta(doc: &Html, attrs: &[(&str, &str)]) -> Option<String> {
    for (attr, value) in attrs {
        let selector_str = format!(r#"meta[{attr}="{value}"]"#);
        if let Ok(sel) = Selector::parse(&selector_str)
            && let Some(el) = doc.select(&sel).next()
            && let Some(content) = el.value().attr("content")
        {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn get_all_meta(doc: &Html, attrs: &[(&str, &str)]) -> Vec<String> {
    let mut results = Vec::new();
    for (attr, value) in attrs {
        let selector_str = format!(r#"meta[{attr}="{value}"]"#);
        let Ok(sel) = Selector::parse(&selector_str) else {
            continue;
        };
        for el in doc.select(&sel) {
            if let Some(content) = el.value().attr("content") {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    results.push(trimmed.to_string());
                }
            }
        }
    }
    results
}

fn extract_year(s: &str) -> Option<i32> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i].is_ascii_digit() {
            if let Ok(year) = s[i..i + 4].parse::<i32>()
                && (1900..=2100).contains(&year)
            {
                return Some(year);
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

#[derive(Debug, Deserialize)]
struct JsonLdItem {
    name: Option<String>,
    headline: Option<String>,
    doi: Option<String>,
    author: Option<serde_json::Value>,
    #[serde(rename = "isPartOf")]
    is_part_of: Option<JsonLdPartOf>,
    #[serde(rename = "datePublished")]
    date_published: Option<String>,
    description: Option<String>,
    publisher: Option<serde_json::Value>,
    pagination: Option<String>,
    #[serde(rename = "volumeNumber")]
    volume_number: Option<serde_json::Value>,
    #[serde(rename = "issueNumber")]
    issue_number: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct JsonLdPartOf {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JsonLdAuthor {
    name: Option<String>,
    #[serde(rename = "givenName")]
    given_name: Option<String>,
    #[serde(rename = "familyName")]
    family_name: Option<String>,
}

fn extract_jsonld(doc: &Html, meta: &mut ScrapedFields) {
    let Ok(sel) = Selector::parse(r#"script[type="application/ld+json"]"#) else {
        return;
    };

    let scholarly_types = [
        "ScholarlyArticle",
        "Article",
        "TechArticle",
        "MedicalScholarlyArticle",
    ];

    for script in doc.select(&sel) {
        let text = script.text().collect::<String>();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };

        let items: Vec<serde_json::Value> = if value.is_array() {
            serde_json::from_value(value).unwrap_or_default()
        } else {
            vec![value]
        };

        for item_val in items {
            let type_val = item_val.get("@type");
            let is_scholarly = match type_val {
                Some(serde_json::Value::String(s)) => scholarly_types.contains(&s.as_str()),
                Some(serde_json::Value::Array(arr)) => arr.iter().any(|v| {
                    v.as_str()
                        .map(|s| scholarly_types.contains(&s))
                        .unwrap_or(false)
                }),
                _ => false,
            };
            if !is_scholarly {
                continue;
            }

            let Ok(item) = serde_json::from_value::<JsonLdItem>(item_val) else {
                continue;
            };

            if meta.title.is_none() {
                meta.title = item.name.or(item.headline);
            }
            if meta.doi.is_none() {
                meta.doi = item.doi;
            }
            if meta.authors.is_empty()
                && let Some(author_val) = item.author
            {
                meta.authors = parse_jsonld_authors(author_val);
            }
            if meta.journal.is_none() {
                meta.journal = item.is_part_of.and_then(|p| p.name);
            }
            if let Some(ref d) = item.date_published {
                if meta.year.is_none() {
                    meta.year = extract_year(d);
                }
                if meta.date.is_none() {
                    meta.date = Some(d.clone());
                }
            }
            if meta.abstract_text.is_none() {
                meta.abstract_text = item.description;
            }
            if meta.publisher.is_none() {
                meta.publisher = match item.publisher {
                    Some(serde_json::Value::String(s)) => Some(s),
                    Some(serde_json::Value::Object(obj)) => {
                        obj.get("name").and_then(|v| v.as_str()).map(String::from)
                    }
                    _ => None,
                };
            }
            if meta.pages.is_none() {
                meta.pages = item.pagination;
            }
            if meta.volume.is_none() {
                meta.volume = item.volume_number.and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
            }
            if meta.issue.is_none() {
                meta.issue = item.issue_number.and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
            }

            return; // Use first scholarly article found
        }
    }
}

fn parse_jsonld_authors(value: serde_json::Value) -> Vec<String> {
    let authors_arr = if value.is_array() {
        serde_json::from_value::<Vec<serde_json::Value>>(value).unwrap_or_default()
    } else {
        vec![value]
    };

    authors_arr
        .into_iter()
        .filter_map(|v| {
            if let Ok(a) = serde_json::from_value::<JsonLdAuthor>(v) {
                let name = a.name.or_else(|| match (a.given_name, a.family_name) {
                    (Some(g), Some(f)) => Some(format!("{g} {f}")),
                    (None, Some(f)) => Some(f),
                    (Some(g), None) => Some(g),
                    (None, None) => None,
                });
                name.filter(|s| !s.is_empty())
            } else {
                None
            }
        })
        .collect()
}

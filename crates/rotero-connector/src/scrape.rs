use rotero_models::{Paper, PaperLinks, Publication};
use scraper::{Html, Selector};
use serde::Deserialize;

pub async fn scrape_url(url: &str) -> Result<Paper, String> {
    // Validate URL scheme to prevent SSRF (file://, internal networks, etc.)
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Unsupported URL scheme: {scheme}")),
    }

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {e}"))?;

    let resp = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; Rotero/0.1; +https://github.com/rotero)",
        )
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {} for {url}", resp.status()));
    }

    let final_url = resp.url().to_string();
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {e}"))?;

    let mut paper = extract_from_html(&html);
    paper.links.url = Some(final_url);
    Ok(paper)
}

#[derive(Debug, Default)]
struct ScrapedFields {
    title: Option<String>,
    authors: Vec<String>,
    doi: Option<String>,
    url: Option<String>,
    pdf_url: Option<String>,
    journal: Option<String>,
    year: Option<i32>,
    volume: Option<String>,
    issue: Option<String>,
    pages: Option<String>,
    publisher: Option<String>,
    abstract_text: Option<String>,
}

impl ScrapedFields {
    fn into_paper(self) -> Paper {
        Paper {
            title: self.title.unwrap_or_else(|| "Untitled".to_string()),
            authors: self.authors,
            year: self.year,
            doi: self.doi,
            abstract_text: self.abstract_text,
            publication: Publication {
                journal: self.journal,
                volume: self.volume,
                issue: self.issue,
                pages: self.pages,
                publisher: self.publisher,
            },
            links: PaperLinks {
                url: self.url,
                pdf_url: self.pdf_url,
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

/// Extracts scholarly metadata from HTML meta tags and JSON-LD.
#[allow(clippy::field_reassign_with_default)] // fields computed incrementally from HTML
pub fn extract_from_html(html: &str) -> Paper {
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
        if let Some(stripped) = doi.strip_prefix("https://doi.org/") {
            *doi = stripped.to_string();
        } else if let Some(stripped) = doi.strip_prefix("http://doi.org/") {
            *doi = stripped.to_string();
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
            ("name", "DC.creator"),
            ("name", "dc.creator"),
            ("name", "eprints.creators_name"),
        ],
    );

    meta.pdf_url = get_meta(&doc, &[("name", "citation_pdf_url")]);

    meta.journal = get_meta(
        &doc,
        &[
            ("name", "citation_journal_title"),
            ("name", "prism.publicationName"),
            ("name", "DC.source"),
            ("name", "dc.source"),
        ],
    );

    let date_str = get_meta(
        &doc,
        &[
            ("name", "citation_publication_date"),
            ("name", "citation_date"),
            ("name", "DC.date"),
            ("name", "dc.date"),
            ("property", "article:published_time"),
        ],
    );
    if let Some(ref d) = date_str
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

    extract_jsonld(&doc, &mut meta);

    meta.into_paper()
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
            if meta.year.is_none()
                && let Some(ref d) = item.date_published
            {
                meta.year = extract_year(d);
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

use axum::{Json, extract::Query, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::ConnectorState;
use rotero_bib::citation::AVAILABLE_STYLES;

/// Incoming JSON body for `POST /api/save`.
#[derive(Debug, Deserialize)]
pub struct SavePaperRequest {
    pub url: Option<String>,
    pub doi: Option<String>,
    pub title: Option<String>,
    pub item_type: Option<String>,
    pub authors: Option<Vec<String>>,
    pub pdf_url: Option<String>,
    pub journal: Option<String>,
    pub year: Option<i32>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub abstract_text: Option<String>,
    pub collection_id: Option<String>,
    pub tag_ids: Option<Vec<String>>,
}

/// JSON response returned by `POST /api/save`.
#[derive(Debug, Serialize)]
pub struct SavePaperResponse {
    pub success: bool,
    pub message: String,
    pub paper_id: Option<String>,
}

/// JSON response for `GET /api/status` health-check.
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub name: &'static str,
}

/// Lightweight collection descriptor sent to the browser extension.
#[derive(Debug, Serialize)]
pub struct CollectionInfo {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// JSON response for `GET /api/collections`.
#[derive(Debug, Serialize)]
pub struct CollectionsResponse {
    pub collections: Vec<CollectionInfo>,
}

/// Lightweight tag descriptor sent to the browser extension.
#[derive(Debug, Serialize)]
pub struct TagInfo {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

/// JSON response for `GET /api/tags`.
#[derive(Debug, Serialize)]
pub struct TagsResponse {
    pub tags: Vec<TagInfo>,
}

/// Handler for `GET /api/status`. Returns app name and version.
pub async fn status() -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        name: "Rotero",
    })
}

/// Handler for `GET /api/collections`. Returns the user's collection list.
pub async fn collections(State(state): State<Arc<ConnectorState>>) -> Json<CollectionsResponse> {
    let collections = if let Some(ref callback) = state.on_get_collections {
        callback().await
    } else {
        Vec::new()
    };
    Json(CollectionsResponse { collections })
}

/// Handler for `GET /api/tags`. Returns the user's tag list.
pub async fn tags(State(state): State<Arc<ConnectorState>>) -> Json<TagsResponse> {
    let tags = if let Some(ref callback) = state.on_get_tags {
        callback().await
    } else {
        Vec::new()
    };
    Json(TagsResponse { tags })
}

/// Incoming JSON body for `POST /api/scrape`.
#[derive(Debug, Deserialize)]
pub struct ScrapeRequest {
    pub url: String,
    /// The page's rendered HTML, captured by the browser extension in the user's
    /// (possibly authenticated) tab. When present, the connector translates it
    /// directly instead of re-fetching server-side — sidestepping publisher
    /// anti-bot walls that would block a bare server fetch.
    #[serde(default)]
    pub html: Option<String>,
    /// Optional *raw server HTML* — a page-context `fetch` of the page's own URL
    /// captured by the extension. It carries inline `<script>` data (IEEE
    /// `global.document.metadata`, `__NEXT_DATA__`, JSON-LD) that an SPA strips
    /// from the rendered [`html`], so JS translators parse against it when present.
    #[serde(default)]
    pub raw_html: Option<String>,
}

/// JSON response for `POST /api/scrape` and `POST /api/scrape/continue`.
///
/// `done` distinguishes the two outcomes of a browser-proxied run: `true` carries
/// the final `metadata` (or a miss), while `false` carries a `fetch` instruction
/// the extension must perform in the authenticated tab, plus the `run_id` to
/// resume under. The one-shot path (no follow-up fetch, or the engine disabled)
/// always returns `done: true`, so an extension that ignores the new fields still
/// works unchanged.
#[derive(Debug, Serialize)]
pub struct ScrapeResponse {
    pub success: bool,
    pub done: bool,
    pub metadata: Option<ScrapeResult>,
    pub error: Option<String>,
    /// Set when `done` is false: the follow-up fetch for the extension to perform.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg(feature = "translator-engine")]
    pub fetch: Option<crate::scrape_session::FetchInstruction>,
    /// Set when `done` is false: the session id to pass to `/api/scrape/continue`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<u64>,
}

impl ScrapeResponse {
    /// A completed response carrying extracted metadata (or a miss).
    fn done(success: bool, metadata: Option<ScrapeResult>, error: Option<String>) -> Self {
        Self {
            success,
            done: true,
            metadata,
            error,
            #[cfg(feature = "translator-engine")]
            fetch: None,
            run_id: None,
        }
    }
}

/// Scraped paper metadata returned inside [`ScrapeResponse`].
#[derive(Debug, Serialize)]
pub struct ScrapeResult {
    pub title: Option<String>,
    pub item_type: String,
    pub authors: Vec<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub pdf_url: Option<String>,
    pub journal: Option<String>,
    pub year: Option<i32>,
    pub volume: Option<String>,
    pub issue: Option<String>,
    pub pages: Option<String>,
    pub publisher: Option<String>,
    pub abstract_text: Option<String>,
}

/// Handler for `POST /api/scrape`. Runs the in-process translators over the
/// page (the corpus JS engine plus the Rust hubs, one of which — Embedded
/// Metadata — is the generic `<meta>`/JSON-LD extractor). A single fetch and a
/// single extraction pass: the site translators, then Embedded Metadata as the
/// always-applicable floor, all in one registry call.
pub async fn scrape(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<ScrapeRequest>,
) -> Json<ScrapeResponse> {
    let html = req.html.as_deref().filter(|h| !h.trim().is_empty());
    let raw = req.raw_html.as_deref().filter(|h| !h.trim().is_empty());

    // With the JS engine and extension-supplied HTML, run the brokered path: a
    // site translator's follow-up fetch (IEEE's citation API, an Atypon RIS POST)
    // is proxied back to the authenticated tab. The run may park mid-way, which
    // yields a `done: false` response the extension continues.
    #[cfg(feature = "translator-engine")]
    if let Some(html) = html {
        return Json(start_brokered_scrape(&state, &req.url, html, raw).await);
    }

    // Otherwise a single non-brokered pass: extension HTML if present (no
    // follow-up proxying), else a server-side fetch.
    let items = match html {
        Some(html) => {
            state
                .translator_registry
                .translate_html_raw(&req.url, html, raw)
                .await
        }
        None => state.translator_registry.translate_web(&req.url).await,
    };
    Json(finish_scrape(&req.url, items.as_deref()))
}

/// Build the terminal response for a completed run: `Hit` with metadata, or
/// `Miss`. Records telemetry.
fn finish_scrape(url: &str, items: Option<&[rotero_translate::ZoteroItem]>) -> ScrapeResponse {
    match items.and_then(best_result) {
        Some(result) => {
            crate::telemetry::record(crate::telemetry::Outcome::Hit, url);
            ScrapeResponse::done(true, Some(result), None)
        }
        None => {
            crate::telemetry::record(crate::telemetry::Outcome::Miss, url);
            ScrapeResponse::done(
                false,
                None,
                Some(format!("no metadata extracted for {url}")),
            )
        }
    }
}

/// Start a browser-proxied translation and drive it to its first park or to
/// completion. If the run finishes without a follow-up fetch, returns a terminal
/// response; if it parks, registers the session and returns a `fetch` for the
/// extension plus the `run_id` to continue under.
#[cfg(feature = "translator-engine")]
async fn start_brokered_scrape(
    state: &Arc<ConnectorState>,
    url: &str,
    html: &str,
    raw_html: Option<&str>,
) -> ScrapeResponse {
    use crate::scrape_session::{Session, Step};

    let (queue_tx, queue_rx) = tokio::sync::mpsc::unbounded_channel();

    // Spawn the translation, routing its follow-up fetches to `queue_tx`. The
    // task owns a clone of the shared state so it can outlive this request while
    // the session is parked between round-trips.
    let task_state = Arc::clone(state);
    let url_owned = url.to_string();
    let html_owned = html.to_string();
    let raw_owned = raw_html.map(str::to_string);
    let join = tokio::spawn(async move {
        task_state
            .translator_registry
            .translate_html_brokered(&url_owned, &html_owned, raw_owned.as_deref(), queue_tx)
            .await
    });

    let mut session = Session::new(join, queue_rx);
    match session.drive().await {
        Step::Done(items) => finish_scrape(url, items.as_deref()),
        Step::Fetch(fetch) => {
            let run_id = state.scrape_sessions.insert(session);
            ScrapeResponse {
                success: true,
                done: false,
                metadata: None,
                error: None,
                fetch: Some(fetch),
                run_id: Some(run_id),
            }
        }
    }
}

/// JSON body for `POST /api/scrape/continue`: the extension's outcome for the
/// fetch it was asked to perform, plus the session to resume.
#[cfg(feature = "translator-engine")]
#[derive(Debug, Deserialize)]
pub struct ScrapeContinueRequest {
    pub run_id: u64,
    pub url: String,
    pub response: crate::scrape_session::FetchOutcome,
}

/// Handler for `POST /api/scrape/continue`. Feeds the extension's fetched body to
/// the parked engine and drives it to the next park or to completion.
#[cfg(feature = "translator-engine")]
pub async fn scrape_continue(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<ScrapeContinueRequest>,
) -> Json<ScrapeResponse> {
    use crate::scrape_session::Step;

    let Some(mut session) = state.scrape_sessions.take(req.run_id) else {
        return Json(ScrapeResponse::done(
            false,
            None,
            Some(format!("unknown scrape session {}", req.run_id)),
        ));
    };

    if !session.resume(req.response) {
        // No fetch was awaiting a reply — a stray or duplicate continue. Put the
        // session back so a legitimate follow-up can still resume it.
        state.scrape_sessions.put_back(req.run_id, session);
        return Json(ScrapeResponse::done(
            false,
            None,
            Some("no pending fetch for this session".to_string()),
        ));
    }

    match session.drive().await {
        Step::Done(items) => Json(finish_scrape(&req.url, items.as_deref())),
        Step::Fetch(fetch) => {
            state.scrape_sessions.put_back(req.run_id, session);
            Json(ScrapeResponse {
                success: true,
                done: false,
                metadata: None,
                error: None,
                fetch: Some(fetch),
                run_id: Some(req.run_id),
            })
        }
    }
}

/// Pick the best [`ScrapeResult`] from a translator's items: the first usable
/// (non-note, non-attachment, titled) record. Returns `None` if the list has
/// nothing bibliographic.
fn best_result(items: &[rotero_translate::ZoteroItem]) -> Option<ScrapeResult> {
    let item = items
        .iter()
        .find(|i| i.item_type != "note" && i.item_type != "attachment" && !i.title.is_empty())?;
    let pdf_url = item.pdf_url();
    let paper = item.clone().into_paper()?;
    Some(paper_to_result(&paper, pdf_url))
}

/// Convert a [`Paper`](rotero_models::Paper) into a [`ScrapeResult`].
fn paper_to_result(p: &rotero_models::Paper, pdf_url: Option<String>) -> ScrapeResult {
    ScrapeResult {
        title: Some(p.title.clone()),
        item_type: p.item_type.clone(),
        authors: p.author_names(),
        doi: p.doi.clone(),
        url: p.links.url.clone(),
        pdf_url,
        journal: p.publication.journal.clone(),
        year: p.year,
        volume: p.publication.volume.clone(),
        issue: p.publication.issue.clone(),
        pages: p.publication.pages.clone(),
        publisher: p.publication.publisher.clone(),
        abstract_text: p.abstract_text.clone(),
    }
}

// ---------------------------------------------------------------------------
// Citation API types
// ---------------------------------------------------------------------------

/// A style entry returned by `GET /api/cite/styles`.
#[derive(Debug, Serialize)]
pub struct StyleEntry {
    pub id: String,
    pub name: String,
}

/// JSON response for `GET /api/cite/styles`.
#[derive(Debug, Serialize)]
pub struct StylesResponse {
    pub styles: Vec<StyleEntry>,
}

/// Query parameter for `GET /api/cite/search`.
#[derive(Debug, Deserialize)]
pub struct CiteSearchQuery {
    pub q: String,
}

/// Lightweight paper info returned by search.
#[derive(Debug, Serialize)]
pub struct CiteSearchPaper {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i32>,
    pub doi: Option<String>,
    pub journal: Option<String>,
}

/// JSON response for `GET /api/cite/search`.
#[derive(Debug, Serialize)]
pub struct CiteSearchResponse {
    pub papers: Vec<CiteSearchPaper>,
}

/// JSON request body for `POST /api/cite/format`.
#[derive(Debug, Deserialize)]
pub struct CiteFormatRequest {
    pub paper_ids: Vec<String>,
    pub style: String,
}

/// Per-paper citation text.
#[derive(Debug, Serialize)]
pub struct CitationEntry {
    pub paper_id: String,
    pub text: String,
}

/// JSON response for `POST /api/cite/format`.
#[derive(Debug, Serialize)]
pub struct CiteFormatResponse {
    pub success: bool,
    pub citations: Vec<CitationEntry>,
    pub combined: String,
    pub error: Option<String>,
}

/// JSON request body for `POST /api/cite/bibliography`.
#[derive(Debug, Deserialize)]
pub struct CiteBibliographyRequest {
    pub paper_ids: Vec<String>,
    pub style: String,
}

/// JSON response for `POST /api/cite/bibliography`.
#[derive(Debug, Serialize)]
pub struct CiteBibliographyResponse {
    pub success: bool,
    pub entries: Vec<CitationEntry>,
    pub error: Option<String>,
}

/// Convert a display name to a URL-safe slug (e.g. "APA 7th" → "apa-7th").
fn style_slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// Resolve a style slug back to an `ArchivedStyle`.
fn resolve_style(slug: &str) -> Option<rotero_bib::ArchivedStyle> {
    AVAILABLE_STYLES
        .iter()
        .find(|(name, _)| style_slug(name) == slug)
        .map(|(_, style)| *style)
}

/// Handler for `GET /api/cite/styles`. Returns all available CSL styles.
pub async fn cite_styles() -> Json<StylesResponse> {
    let styles = AVAILABLE_STYLES
        .iter()
        .map(|(name, _)| StyleEntry {
            id: style_slug(name),
            name: name.to_string(),
        })
        .collect();
    Json(StylesResponse { styles })
}

/// Handler for `GET /api/cite/search?q=...`. Searches papers in the library.
pub async fn cite_search(
    State(state): State<Arc<ConnectorState>>,
    Query(query): Query<CiteSearchQuery>,
) -> Json<CiteSearchResponse> {
    let papers = if let Some(ref callback) = state.on_search_papers {
        callback(query.q).await
    } else {
        Vec::new()
    };
    let results = papers
        .into_iter()
        .filter_map(|p| {
            let authors = p.author_names();
            Some(CiteSearchPaper {
                id: p.id?,
                title: p.title,
                authors,
                year: p.year,
                doi: p.doi,
                journal: p.publication.journal,
            })
        })
        .collect();
    Json(CiteSearchResponse { papers: results })
}

/// Handler for `POST /api/cite/format`. Returns inline citation text.
pub async fn cite_format(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<CiteFormatRequest>,
) -> Json<CiteFormatResponse> {
    let style = match resolve_style(&req.style) {
        Some(s) => s,
        None => {
            return Json(CiteFormatResponse {
                success: false,
                citations: Vec::new(),
                combined: String::new(),
                error: Some(format!("Unknown style: {}", req.style)),
            });
        }
    };

    let papers = if let Some(ref callback) = state.on_get_papers_by_ids {
        callback(req.paper_ids.clone()).await
    } else {
        Vec::new()
    };

    match rotero_bib::format_inline_citations(&papers, style) {
        Ok((individual, combined)) => {
            let citations = papers
                .iter()
                .zip(individual)
                .filter_map(|(p, text)| {
                    Some(CitationEntry {
                        paper_id: p.id.clone()?,
                        text,
                    })
                })
                .collect();
            Json(CiteFormatResponse {
                success: true,
                citations,
                combined,
                error: None,
            })
        }
        Err(e) => Json(CiteFormatResponse {
            success: false,
            citations: Vec::new(),
            combined: String::new(),
            error: Some(e),
        }),
    }
}

/// Handler for `POST /api/cite/bibliography`. Returns formatted bibliography entries.
pub async fn cite_bibliography(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<CiteBibliographyRequest>,
) -> Json<CiteBibliographyResponse> {
    let style = match resolve_style(&req.style) {
        Some(s) => s,
        None => {
            return Json(CiteBibliographyResponse {
                success: false,
                entries: Vec::new(),
                error: Some(format!("Unknown style: {}", req.style)),
            });
        }
    };

    let papers = if let Some(ref callback) = state.on_get_papers_by_ids {
        callback(req.paper_ids.clone()).await
    } else {
        Vec::new()
    };

    match rotero_bib::format_bibliography_entries(&papers, style) {
        Ok(texts) => {
            let entries = papers
                .iter()
                .zip(texts)
                .filter_map(|(p, text)| {
                    Some(CitationEntry {
                        paper_id: p.id.clone()?,
                        text,
                    })
                })
                .collect();
            Json(CiteBibliographyResponse {
                success: true,
                entries,
                error: None,
            })
        }
        Err(e) => Json(CiteBibliographyResponse {
            success: false,
            entries: Vec::new(),
            error: Some(e),
        }),
    }
}

/// Handler for `POST /api/save`. Creates a [`Paper`](rotero_models::Paper) and
/// invokes the `on_paper_saved` callback to persist it.
pub async fn save_paper(
    State(state): State<Arc<ConnectorState>>,
    Json(req): Json<SavePaperRequest>,
) -> Json<SavePaperResponse> {
    let paper = rotero_models::Paper {
        title: req.title.unwrap_or_else(|| "Untitled".to_string()),
        item_type: req
            .item_type
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| rotero_models::Paper::default().item_type),
        creators: req
            .authors
            .unwrap_or_default()
            .iter()
            .map(|a| rotero_models::Creator::author_from_display(a))
            .collect(),
        year: req.year,
        doi: req.doi,
        abstract_text: req.abstract_text,
        publication: rotero_models::Publication {
            journal: req.journal,
            volume: req.volume,
            issue: req.issue,
            pages: req.pages,
            publisher: req.publisher,
            ..Default::default()
        },
        links: rotero_models::PaperLinks {
            url: req.url,
            ..Default::default()
        },
        ..Default::default()
    };

    if let Some(ref callback) = state.on_paper_saved {
        callback(
            paper.clone(),
            req.collection_id,
            req.tag_ids.unwrap_or_default(),
            req.pdf_url,
        )
        .await;
    }

    Json(SavePaperResponse {
        success: true,
        message: "Paper added to library".to_string(),
        paper_id: paper.id,
    })
}

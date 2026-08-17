use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use rotero_models::{Annotation, Collection, Paper, Tag};
use rotero_pdf::{BookmarkEntry, PageTextData, RenderedPage, SearchMatch};

pub type TabId = u64;

#[derive(Debug, Clone, Default)]
pub struct PageRenderData {
    /// Resident rendered pages keyed by absolute page index. Bounded to a
    /// sliding window around the viewport (see `MAX_RESIDENT_PAGES`); BTreeMap
    /// so iteration yields pages in page order for the render loop.
    pub rendered_pages: BTreeMap<u32, RenderedPageData>,
    /// Pixel dimensions `(width, height)` for every page, indexed by page number.
    /// Used to size placeholders for not-yet-rendered pages so the scroll
    /// container has its full height and scrolling can reach every page.
    pub page_dims: Vec<(u32, u32)>,
    pub text_data: HashMap<u32, PageTextData>,
    pub thumbnails: HashMap<u32, RenderedPageData>,
}

#[derive(Debug, Clone)]
pub struct ViewState {
    pub zoom: f32,
    pub render_zoom: f32,
    pub scroll_top: f64,
    pub page_batch_size: u32,
    /// Render at `zoom * dpr` for sharp HiDPI output.
    pub dpr: f32,
    /// Page index nearest the viewport center, reported by the viewer's
    /// scroll handler. Drives the sliding render window.
    pub current_page: u32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.5,
            render_zoom: 1.5,
            scroll_top: 0.0,
            page_batch_size: 5,
            dpr: 1.0,
            current_page: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub visible: bool,
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current_index: usize,
}

#[derive(Debug, Clone, Default)]
pub struct NavPanels {
    pub show_thumbnails: bool,
    pub show_outline: bool,
    pub outline: Vec<BookmarkEntry>,
}

/// Where a [`PageLink`] jumps to: an in-document location or an external URI.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkDest {
    /// Jump to another page in this document, optionally at a fractional y
    /// position (0..1) down the target page. `None` jumps to the page top.
    Internal { page: u32, y_frac: Option<f64> },
    /// Open an external resource (web URL, DOI, etc.).
    External { uri: String },
}

/// A clickable link on a page, ready for overlay rendering.
///
/// The source rectangle is stored as fractions of the page's width/height so
/// the overlay can scale it to the rendered image at any zoom (mirrors how the
/// text layer positions glyphs by percentage).
#[derive(Debug, Clone)]
pub struct PageLink {
    /// Left edge, as a fraction (0..1) of page width.
    pub x_frac: f64,
    /// Top edge, as a fraction (0..1) of page height.
    pub y_frac: f64,
    /// Width, as a fraction (0..1) of page width.
    pub w_frac: f64,
    /// Height, as a fraction (0..1) of page height.
    pub h_frac: f64,
    /// The link's destination.
    pub dest: LinkDest,
}

/// Converts extracted links (PDF-point space, y-up) into per-page [`PageLink`]s
/// (fractional, y-down) keyed by source page index.
pub fn build_page_links(links: &[rotero_pdf::ExtractedLink]) -> HashMap<u32, Vec<PageLink>> {
    let mut map: HashMap<u32, Vec<PageLink>> = HashMap::new();
    for l in links {
        if l.page_width_pts <= 0.0 || l.page_height_pts <= 0.0 {
            continue;
        }
        let [left, bottom, right, top] = l.rect_pts;
        let pw = l.page_width_pts as f64;
        let ph = l.page_height_pts as f64;
        // PDF points are y-up; flip to y-down for screen coordinates.
        let x_frac = (left.min(right) as f64 / pw).clamp(0.0, 1.0);
        let w_frac = ((right - left).abs() as f64 / pw).clamp(0.0, 1.0);
        let y_frac = ((ph - top.max(bottom) as f64) / ph).clamp(0.0, 1.0);
        let h_frac = ((top - bottom).abs() as f64 / ph).clamp(0.0, 1.0);
        let dest = match &l.target {
            rotero_pdf::LinkTarget::Internal { page, y_frac } => LinkDest::Internal {
                page: *page,
                y_frac: y_frac.map(|f| f as f64),
            },
            rotero_pdf::LinkTarget::External { uri } => LinkDest::External { uri: uri.clone() },
        };
        map.entry(l.page).or_default().push(PageLink {
            x_frac,
            y_frac,
            w_frac,
            h_frac,
            dest,
        });
    }
    map
}

#[derive(Debug, Clone)]
pub struct PdfTab {
    pub id: TabId,
    pub pdf_path: String,
    pub paper_id: Option<String>,
    pub title: String,
    pub page_count: u32,
    pub is_loading: bool,
    /// Why the document could not be opened, if it could not.
    ///
    /// Without this a failed open left `is_loading` set with nothing to show:
    /// the viewer only retries while `is_loading && rendered_pages.is_empty()`,
    /// and the tab-bar path needs `page_count > 0`, so neither fired and the
    /// spinner ran until the tab was closed.
    pub load_error: Option<String>,
    pub is_suspended: bool,

    pub render: PageRenderData,
    pub view: ViewState,
    pub search: SearchState,
    pub nav: NavPanels,
    pub annotations: Vec<Annotation>,
    /// Clickable intra-document links, keyed by source page index. Extracted
    /// once when the tab opens.
    pub links: HashMap<u32, Vec<PageLink>>,
}

impl PdfTab {
    pub fn new(
        id: TabId,
        pdf_path: String,
        title: String,
        zoom: f32,
        batch_size: u32,
        dpr: f32,
    ) -> Self {
        Self {
            id,
            pdf_path,
            paper_id: None,
            title,
            page_count: 0,
            is_loading: true,
            load_error: None,
            is_suspended: false,
            render: PageRenderData::default(),
            view: ViewState {
                zoom,
                render_zoom: zoom * dpr,
                page_batch_size: batch_size,
                dpr,
                ..Default::default()
            },
            search: SearchState::default(),
            nav: NavPanels::default(),
            annotations: Vec::new(),
            links: HashMap::new(),
        }
    }

    pub fn needs_render(&self) -> bool {
        self.render.rendered_pages.is_empty() && self.page_count > 0
    }

    pub fn suspend(&mut self) {
        self.is_suspended = true;
        self.render.rendered_pages.clear();
    }
}

#[derive(Debug, Clone)]
pub struct PdfTabManager {
    pub tabs: Vec<PdfTab>,
    pub active_tab_id: Option<TabId>,
    next_id: u64,
    max_resident: u32,
}

impl Default for PdfTabManager {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active_tab_id: None,
            next_id: 0,
            max_resident: 3,
        }
    }
}

impl PdfTabManager {
    pub fn next_id(&mut self) -> TabId {
        self.next_id += 1;
        self.next_id
    }

    pub fn find_by_paper_id(&self, paper_id: &str) -> Option<usize> {
        self.tabs
            .iter()
            .position(|t| t.paper_id.as_deref() == Some(paper_id))
    }

    pub fn find_by_path(&self, path: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.pdf_path == path)
    }

    pub fn active_tab(&self) -> Option<&PdfTab> {
        self.active_tab_id
            .and_then(|id| self.tabs.iter().find(|t| t.id == id))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut PdfTab> {
        self.active_tab_id
            .and_then(|id| self.tabs.iter_mut().find(|t| t.id == id))
    }

    /// Panics if no active tab.
    pub fn tab(&self) -> &PdfTab {
        self.active_tab().expect("no active tab")
    }

    pub fn tab_mut(&mut self) -> &mut PdfTab {
        self.active_tab_mut().expect("no active tab")
    }

    pub fn set_max_resident(&mut self, max_resident: u32) {
        self.max_resident = max_resident;
    }

    pub fn switch_to(&mut self, tab_id: TabId) {
        self.active_tab_id = Some(tab_id);
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.is_suspended = false;
        }

        let resident: Vec<TabId> = self
            .tabs
            .iter()
            .filter(|t| t.id != tab_id && !t.render.rendered_pages.is_empty())
            .map(|t| t.id)
            .collect();

        let limit = self.max_resident.max(1) as usize;
        if resident.len() >= limit {
            let to_suspend = resident.len() - (limit - 1);
            for &id in resident.iter().take(to_suspend) {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) {
                    tab.suspend();
                }
            }
        }
    }

    pub fn close_tab(&mut self, tab_id: TabId) -> bool {
        let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) else {
            return false;
        };
        self.tabs.remove(idx);
        if self.active_tab_id == Some(tab_id) {
            self.active_tab_id = if self.tabs.is_empty() {
                None
            } else {
                Some(self.tabs[idx.min(self.tabs.len() - 1)].id)
            };
            return true;
        }
        false
    }

    pub fn close_others(&mut self, keep_id: TabId) {
        self.tabs.retain(|t| t.id == keep_id);
        self.active_tab_id = Some(keep_id);
    }

    pub fn close_to_right(&mut self, tab_id: TabId) {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.truncate(idx + 1);
            if let Some(active) = self.active_tab_id
                && !self.tabs.iter().any(|t| t.id == active)
            {
                self.active_tab_id = Some(tab_id);
            }
        }
    }

    pub fn open_tab(&mut self, tab: PdfTab) -> TabId {
        let tab_id = tab.id;
        self.tabs.push(tab);
        self.switch_to(tab_id);
        tab_id
    }

    pub fn open_or_switch(
        &mut self,
        paper_id: String,
        pdf_path: String,
        title: String,
        zoom: f32,
        batch_size: u32,
        dpr: f32,
    ) {
        if let Some(idx) = self.find_by_paper_id(&paper_id) {
            let tid = self.tabs[idx].id;
            self.switch_to(tid);
        } else {
            let id = self.next_id();
            let mut tab = PdfTab::new(id, pdf_path, title, zoom, batch_size, dpr);
            tab.paper_id = Some(paper_id);
            self.open_tab(tab);
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViewerToolState {
    pub annotation_mode: AnnotationMode,
    pub annotation_color: String,
    pub show_annotation_panel: bool,
}

impl Default for ViewerToolState {
    fn default() -> Self {
        Self {
            annotation_mode: AnnotationMode::None,
            annotation_color: "#ffff00".to_string(),
            show_annotation_panel: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum AnnotationMode {
    #[default]
    None,
    Highlight,
    Note,
    Underline,
    Ink,
    Text,
}

/// Uses `Arc<String>` for near-free cloning during Dioxus render cycles.
#[derive(Debug, Clone)]
pub struct RenderedPageData {
    pub page_index: u32,
    pub base64_data: Arc<String>,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

impl From<RenderedPage> for RenderedPageData {
    fn from(rp: RenderedPage) -> Self {
        Self {
            page_index: rp.page_index,
            base64_data: Arc::new(rp.base64_data),
            mime: rp.mime,
            width: rp.width,
            height: rp.height,
        }
    }
}

/// One online provider's slice of a search. Owns its own results and in-flight
/// flag so it can render the instant its response lands, independent of the
/// other providers.
#[derive(Debug, Clone, Default)]
pub struct ProviderResults {
    /// `None` = not yet returned for the current query; `Some(vec)` = returned
    /// (possibly empty).
    pub results: Option<Vec<Paper>>,
    pub searching: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LibrarySearchState {
    pub query: String,
    /// Bumped once per committed (post-debounce) query. Every async search task
    /// captures the generation it fired under and only writes its slice if it
    /// still matches, so a slow response from an older query can never overwrite
    /// results for a newer one.
    pub generation: u64,
    /// Local DB results.
    pub results: Option<Vec<Paper>>,
    // Online providers, in fixed order (== render order).
    pub openalex: ProviderResults,
    pub arxiv: ProviderResults,
    pub semantic_scholar: ProviderResults,
}

impl LibrarySearchState {
    /// True when the user has an active query, driving the two-section search
    /// view instead of the normal library view.
    pub fn is_active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// True while any online provider is still in flight.
    pub fn web_searching(&self) -> bool {
        self.openalex.searching || self.arxiv.searching || self.semantic_scholar.searching
    }

    /// Clears every result slice and in-flight flag.
    pub fn reset_results(&mut self) {
        self.results = None;
        self.openalex = ProviderResults::default();
        self.arxiv = ProviderResults::default();
        self.semantic_scholar = ProviderResults::default();
    }
}

#[derive(Debug, Clone, Default)]
pub struct LibraryFilterState {
    pub collection_paper_ids: Option<Vec<String>>,
    pub tag_paper_ids: Option<Vec<String>>,
    pub duplicate_groups: Option<Vec<Vec<Paper>>>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum SortField {
    #[default]
    DateAdded,
    DateModified,
    Title,
    Year,
    FirstAuthor,
    CitationCount,
}

impl SortField {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DateAdded => "Date Added",
            Self::DateModified => "Date Modified",
            Self::Title => "Title",
            Self::Year => "Year",
            Self::FirstAuthor => "First Author",
            Self::CitationCount => "Citations",
        }
    }

    pub fn all() -> &'static [SortField] {
        &[
            SortField::DateAdded,
            SortField::DateModified,
            SortField::Title,
            SortField::Year,
            SortField::FirstAuthor,
            SortField::CitationCount,
        ]
    }

    pub fn default_ascending(&self) -> bool {
        match self {
            Self::Title | Self::FirstAuthor => true,
            Self::DateAdded | Self::DateModified | Self::Year | Self::CitationCount => false,
        }
    }
}

/// A message shown briefly in the corner of the window.
///
/// The app had no way at all to tell the user something went wrong outside of
/// startup, which is why roughly twenty failing writes were discarded with
/// `let _ =` — reporting them would have meant a button that silently did
/// nothing instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Monotonic id, so the list can be keyed and dismissed precisely.
    pub id: u64,
    pub message: String,
    /// Errors persist until dismissed; confirmations time out.
    pub is_error: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub papers: Vec<Paper>,
    pub collections: Vec<Collection>,
    pub tags: Vec<Tag>,
    /// Transient messages awaiting display. See [`Toast`].
    pub toasts: Vec<Toast>,
    /// Source of [`Toast::id`].
    pub next_toast_id: u64,
    pub selected_paper_ids: HashSet<String>,
    pub anchor_paper_id: Option<String>,
    /// A web search result being previewed in the detail panel. Mutually
    /// exclusive with a library-paper selection: previewing a web hit clears
    /// the selection, and selecting a library paper clears this.
    pub previewed_web: Option<Paper>,
    pub confirm_delete: Option<Vec<String>>,
    pub view: LibraryView,
    pub search: LibrarySearchState,
    pub filter: LibraryFilterState,
    pub saved_searches: Vec<rotero_models::SavedSearch>,
    pub sort_field: SortField,
    pub sort_ascending: bool,
    /// In-flight and failed imports, keyed the same way search-result rows are
    /// (`result_key`), so a row can find its own state without a DOI.
    ///
    /// Successful imports are removed once the paper lands in `papers`, which
    /// the result list already treats as "imported" — keeping them here too
    /// would give that fact two sources of truth.
    pub import_status: HashMap<String, ImportStatus>,
}

/// Where a queued import has got to.
///
/// Without this the result list infers everything from DOI membership in
/// `papers`, so a queued, running, or failed import is indistinguishable from
/// one never started: the button stays live and a second click duplicates the
/// work.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportStatus {
    /// Accepted, waiting on a concurrency slot.
    Queued,
    /// Fetching metadata and writing the library row.
    Importing,
    /// Row is saved; fetching the open-access PDF.
    Downloading,
    /// Failed, with a message worth showing the user.
    Failed(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LibraryView {
    #[default]
    AllPapers,
    RecentlyAdded,
    Favorites,
    Unread,
    Collection(String),
    Tag(String),
    Duplicates,
    SavedSearch(String),
    PdfViewer,
    Graph,
}

impl LibraryState {
    /// Report a failed operation to the user.
    ///
    /// Writes that could fail used to be discarded with `let _ =`, because the
    /// alternative was a button that did nothing with no explanation. This is
    /// the missing half of that trade.
    pub fn report_error(&mut self, message: impl Into<String>) {
        let message = message.into();
        tracing::error!("{message}");
        self.push_toast(message, true);
    }

    fn push_toast(&mut self, message: String, is_error: bool) {
        // Repeating the same failure (a settings pane that saves on every
        // keystroke, say) should not stack up identical messages.
        if self.toasts.iter().any(|t| t.message == message) {
            return;
        }
        self.next_toast_id += 1;
        self.toasts.push(Toast {
            id: self.next_toast_id,
            message,
            is_error,
        });
    }

    /// Remove a toast the user dismissed, or one that timed out.
    pub fn dismiss_toast(&mut self, id: u64) {
        self.toasts.retain(|t| t.id != id);
    }

    /// Returns the single selected paper when exactly one is selected.
    pub fn selected_paper(&self) -> Option<&Paper> {
        if self.selected_paper_ids.len() != 1 {
            return None;
        }
        let id = self.selected_paper_ids.iter().next().unwrap();
        self.papers
            .iter()
            .find(|p| p.id.as_deref() == Some(id.as_str()))
    }

    /// Returns the single selected paper ID if exactly one is selected.
    pub fn single_selected_id(&self) -> Option<&String> {
        if self.selected_paper_ids.len() == 1 {
            self.selected_paper_ids.iter().next()
        } else {
            None
        }
    }

    pub fn is_selected(&self, id: &str) -> bool {
        self.selected_paper_ids.contains(id)
    }

    pub fn select_one(&mut self, id: String) {
        self.previewed_web = None;
        self.selected_paper_ids.clear();
        self.selected_paper_ids.insert(id.clone());
        self.anchor_paper_id = Some(id);
    }

    pub fn toggle_select(&mut self, id: &str) {
        self.previewed_web = None;
        if self.selected_paper_ids.contains(id) {
            self.selected_paper_ids.remove(id);
        } else {
            self.selected_paper_ids.insert(id.to_string());
        }
        self.anchor_paper_id = Some(id.to_string());
    }

    /// Show a web search result in the detail panel. Clears any library-paper
    /// selection so the two detail surfaces stay mutually exclusive.
    pub fn preview_web(&mut self, paper: Paper) {
        self.selected_paper_ids.clear();
        self.anchor_paper_id = None;
        self.previewed_web = Some(paper);
    }

    pub fn range_select(&mut self, target_id: &str, ordered_ids: &[String]) {
        self.previewed_web = None;
        let anchor = self.anchor_paper_id.as_deref().unwrap_or(target_id);
        let anchor_pos = ordered_ids.iter().position(|id| id == anchor);
        let target_pos = ordered_ids.iter().position(|id| id == target_id);
        if let (Some(a), Some(t)) = (anchor_pos, target_pos) {
            let (start, end) = if a <= t { (a, t) } else { (t, a) };
            for id in &ordered_ids[start..=end] {
                self.selected_paper_ids.insert(id.clone());
            }
        } else {
            self.select_one(target_id.to_string());
        }
    }

    pub fn select_all(&mut self, ids: impl Iterator<Item = String>) {
        self.selected_paper_ids = ids.collect();
    }

    pub fn clear_selection(&mut self) {
        self.selected_paper_ids.clear();
        self.anchor_paper_id = None;
        self.previewed_web = None;
    }

    pub fn selection_count(&self) -> usize {
        self.selected_paper_ids.len()
    }

    pub fn selected_papers(&self) -> Vec<&Paper> {
        self.papers
            .iter()
            .filter(|p| {
                p.id.as_deref()
                    .is_some_and(|id| self.selected_paper_ids.contains(id))
            })
            .collect()
    }

    /// Search results with each row refreshed from the live library.
    ///
    /// `search.results` is a snapshot taken when the query ran, and no mutation
    /// path updates it — every handler writes to `self.papers`. Rendering the
    /// snapshot directly meant that while a query was active, favouriting a
    /// paper updated the database and then appeared to revert, and a deleted
    /// paper stayed on screen. Worse, the context menu reads `self.papers`, so
    /// the row and its menu disagreed at the same moment.
    ///
    /// The snapshot still decides which papers appear and in what order — that
    /// is the search ranking — but the values shown come from the live list, so
    /// every existing mutation shows through with nothing to keep in sync. Rows
    /// deleted since the query drop out.
    pub fn resolved_search_results(&self, results: &[Paper]) -> Vec<Paper> {
        results
            .iter()
            .filter_map(|hit| {
                let Some(id) = hit.id.as_ref() else {
                    // A web result that is not in the library has no id and
                    // nothing to refresh from; show it as returned.
                    return Some(hit.clone());
                };
                self.papers
                    .iter()
                    .find(|p| p.id.as_ref() == Some(id))
                    .cloned()
            })
            .collect()
    }

    /// Returns the ordered list of paper IDs for the current filtered view.
    pub fn filtered_paper_ids(&self) -> Vec<String> {
        let mut filtered: Vec<_> = if let Some(ref results) = self.search.results {
            self.resolved_search_results(results)
        } else {
            match &self.view {
                LibraryView::AllPapers => self.papers.clone(),
                LibraryView::RecentlyAdded => self.papers.iter().take(20).cloned().collect(),
                LibraryView::Favorites => self
                    .papers
                    .iter()
                    .filter(|p| p.status.is_favorite)
                    .cloned()
                    .collect(),
                LibraryView::Unread => self
                    .papers
                    .iter()
                    .filter(|p| !p.status.is_read)
                    .cloned()
                    .collect(),
                LibraryView::Collection(_) => {
                    if let Some(ref ids) = self.filter.collection_paper_ids {
                        self.papers
                            .iter()
                            .filter(|p| p.id.as_ref().is_some_and(|pid| ids.contains(pid)))
                            .cloned()
                            .collect()
                    } else {
                        Vec::new()
                    }
                }
                LibraryView::Tag(_) => {
                    if let Some(ref ids) = self.filter.tag_paper_ids {
                        self.papers
                            .iter()
                            .filter(|p| p.id.as_ref().is_some_and(|pid| ids.contains(pid)))
                            .cloned()
                            .collect()
                    } else {
                        Vec::new()
                    }
                }
                LibraryView::Duplicates => {
                    if let Some(ref groups) = self.filter.duplicate_groups {
                        groups.iter().flatten().cloned().collect()
                    } else {
                        Vec::new()
                    }
                }
                _ => self.papers.clone(),
            }
        };
        let is_searching = self.search.results.is_some();
        if !is_searching && !matches!(self.view, LibraryView::Duplicates) {
            self.sort_papers(&mut filtered);
        }
        filtered.iter().filter_map(|p| p.id.clone()).collect()
    }

    pub fn sort_papers(&self, papers: &mut [Paper]) {
        let ascending = self.sort_ascending;
        match self.sort_field {
            SortField::Title => {
                papers.sort_by_cached_key(|p| p.title.to_lowercase());
                if !ascending {
                    papers.reverse();
                }
            }
            SortField::FirstAuthor => {
                papers.sort_by_cached_key(|p| {
                    p.author_names()
                        .first()
                        .map(|s| s.to_lowercase())
                        .unwrap_or_default()
                });
                if !ascending {
                    papers.reverse();
                }
            }
            _ => {
                papers.sort_by(|a, b| {
                    let ord = match self.sort_field {
                        SortField::DateAdded => a.status.date_added.cmp(&b.status.date_added),
                        SortField::DateModified => {
                            a.status.date_modified.cmp(&b.status.date_modified)
                        }
                        SortField::Year => a.year.cmp(&b.year),
                        SortField::CitationCount => {
                            a.citation.citation_count.cmp(&b.citation.citation_count)
                        }
                        SortField::Title | SortField::FirstAuthor => unreachable!(),
                    };
                    if ascending { ord } else { ord.reverse() }
                });
            }
        }
    }

    pub fn touch_paper(&mut self, paper_id: &str) {
        if let Some(p) = self
            .papers
            .iter_mut()
            .find(|p| p.id.as_deref() == Some(paper_id))
        {
            p.status.date_modified = chrono::Utc::now();
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnnotationContextInfo {
    pub annotation_id: String,
    pub ann_type: rotero_models::AnnotationType,
    pub page: i32,
    pub color: String,
    pub content: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct DragPaper(pub Option<Vec<String>>);

/// Bumped whenever a paper's tag/collection membership changes (add/remove) from
/// anywhere — the overview toggles, the context menu, drag-and-drop. Two things
/// observe it: the overview panel's chip list (to reflect the paper's own
/// membership) and the library panel's filter effect (to re-query the
/// tag/collection-filtered list, which is otherwise cached per view and would go
/// stale). Membership isn't cached on `Paper` or in `LibraryState`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MembershipRefresh(pub u64);

#[cfg(test)]
mod tests {
    use super::*;

    fn paper(id: &str, title: &str, favorite: bool) -> Paper {
        Paper {
            id: Some(id.to_string()),
            title: title.to_string(),
            status: rotero_models::LibraryStatus {
                is_favorite: favorite,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Search results are a snapshot taken when the query ran, and no mutation
    /// path updates it. Reading through to `papers` is what stops a favourite
    /// toggled during a search from appearing to revert.
    #[test]
    fn search_results_show_current_values_not_the_snapshot() {
        let state = LibraryState {
            papers: vec![paper("p1", "Renamed", true)],
            ..Default::default()
        };

        // The snapshot still holds the values from when the search ran.
        let snapshot = vec![paper("p1", "Original", false)];

        let resolved = state.resolved_search_results(&snapshot);
        assert_eq!(resolved.len(), 1);
        assert!(
            resolved[0].status.is_favorite,
            "the live favourite state must win over the snapshot"
        );
        assert_eq!(resolved[0].title, "Renamed");
    }

    /// A paper deleted while a query is active must leave the results.
    #[test]
    fn a_deleted_paper_drops_out_of_search_results() {
        let state = LibraryState {
            papers: vec![paper("p2", "Kept", false)],
            ..Default::default()
        };

        let snapshot = vec![paper("p1", "Deleted", false), paper("p2", "Kept", false)];

        let resolved = state.resolved_search_results(&snapshot);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].id.as_deref(), Some("p2"));
    }

    /// Ranking order comes from the search, not from the library list.
    #[test]
    fn the_search_ordering_is_preserved() {
        let state = LibraryState {
            papers: vec![paper("a", "A", false), paper("b", "B", false)],
            ..Default::default()
        };

        let snapshot = vec![paper("b", "B", false), paper("a", "A", false)];

        let ids: Vec<_> = state
            .resolved_search_results(&snapshot)
            .into_iter()
            .filter_map(|p| p.id)
            .collect();
        assert_eq!(ids, vec!["b".to_string(), "a".to_string()]);
    }

    /// Repeated identical failures must not stack up: a settings pane saves on
    /// every keystroke, so one broken write would otherwise queue dozens.
    #[test]
    fn the_same_error_is_not_reported_twice() {
        let mut state = LibraryState::default();
        state.report_error("Settings could not be saved: disk full");
        state.report_error("Settings could not be saved: disk full");

        assert_eq!(state.toasts.len(), 1);
        assert!(state.toasts[0].is_error);
    }

    /// Ids are unique, so dismissing one leaves the rest alone.
    #[test]
    fn dismissing_removes_only_the_named_toast() {
        let mut state = LibraryState::default();
        state.report_error("first");
        state.report_error("second");
        let first_id = state.toasts[0].id;

        state.dismiss_toast(first_id);

        assert_eq!(state.toasts.len(), 1);
        assert_eq!(state.toasts[0].message, "second");
    }

    /// A web hit that is not in the library has no id to resolve against and
    /// must still render.
    #[test]
    fn a_result_with_no_id_is_kept_as_returned() {
        let state = LibraryState::default();
        let mut hit = paper("unused", "From the web", false);
        hit.id = None;

        let resolved = state.resolved_search_results(&[hit]);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].title, "From the web");
    }
}

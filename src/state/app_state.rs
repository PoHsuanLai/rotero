use std::collections::HashMap;
use std::sync::Arc;

use rotero_models::{Annotation, Collection, Paper, Tag};
use rotero_pdf::{BookmarkEntry, PageTextData, RenderedPage, SearchMatch};

pub type TabId = u64;

#[derive(Debug, Clone, Default)]
pub struct PageRenderData {
    pub rendered_pages: Vec<RenderedPageData>,
    pub text_data: HashMap<u32, PageTextData>,
    pub thumbnails: HashMap<u32, RenderedPageData>,
    pub _page_dimensions: Vec<(f32, f32)>,
}

#[derive(Debug, Clone)]
pub struct ViewState {
    pub zoom: f32,
    pub render_zoom: f32,
    pub scroll_top: f64,
    pub page_batch_size: u32,
    /// Render at `zoom * dpr` for sharp HiDPI output.
    pub dpr: f32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.5,
            render_zoom: 1.5,
            scroll_top: 0.0,
            page_batch_size: 5,
            dpr: 1.0,
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

#[derive(Debug, Clone)]
pub struct PdfTab {
    pub id: TabId,
    pub pdf_path: String,
    pub paper_id: Option<String>,
    pub title: String,
    pub page_count: u32,
    pub is_loading: bool,
    pub is_suspended: bool,

    pub render: PageRenderData,
    pub view: ViewState,
    pub search: SearchState,
    pub nav: NavPanels,
    pub annotations: Vec<Annotation>,
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
        }
    }

    pub fn needs_render(&self) -> bool {
        self.render.rendered_pages.is_empty() && self.page_count > 0
    }

    pub fn rendered_count(&self) -> u32 {
        self.render.rendered_pages.len() as u32
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

#[derive(Debug, Clone, Default, PartialEq)]
pub enum SearchSource {
    #[default]
    Local,
    OpenAlex,
    ArXiv,
    SemanticScholar,
}

impl SearchSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::OpenAlex => "OpenAlex",
            Self::ArXiv => "arXiv",
            Self::SemanticScholar => "Semantic Scholar",
        }
    }

    pub fn all() -> &'static [SearchSource] {
        &[
            SearchSource::Local,
            SearchSource::OpenAlex,
            SearchSource::ArXiv,
            SearchSource::SemanticScholar,
        ]
    }

    pub fn provider(&self) -> Option<rotero_search::SearchProvider> {
        match self {
            Self::OpenAlex => Some(rotero_search::SearchProvider::OpenAlex),
            Self::ArXiv => Some(rotero_search::SearchProvider::ArXiv),
            Self::SemanticScholar => Some(rotero_search::SearchProvider::SemanticScholar),
            Self::Local => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct LibrarySearchState {
    pub query: String,
    pub results: Option<Vec<Paper>>,
    pub source: SearchSource,
    pub external_results: Option<Vec<Paper>>,
    pub external_searching: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryFilterState {
    pub collection_paper_ids: Option<Vec<String>>,
    pub tag_paper_ids: Option<Vec<String>>,
    pub duplicate_groups: Option<Vec<Vec<Paper>>>,
}

#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub papers: Vec<Paper>,
    pub collections: Vec<Collection>,
    pub tags: Vec<Tag>,
    pub selected_paper_id: Option<String>,
    pub _selected_collection_id: Option<String>,
    pub view: LibraryView,
    pub search: LibrarySearchState,
    pub filter: LibraryFilterState,
    pub saved_searches: Vec<rotero_models::SavedSearch>,
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
    pub fn selected_paper(&self) -> Option<&Paper> {
        self.selected_paper_id
            .as_ref()
            .and_then(|id| self.papers.iter().find(|p| p.id.as_ref() == Some(id)))
    }

    pub fn touch_paper(&mut self, paper_id: &str) {
        if let Some(p) = self.papers.iter_mut().find(|p| p.id.as_deref() == Some(paper_id)) {
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
pub struct DragPaper(pub Option<String>);

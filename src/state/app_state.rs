use std::collections::HashMap;
use std::sync::Arc;

use rotero_models::{Annotation, Collection, Paper, Tag};
use rotero_pdf::{BookmarkEntry, PageTextData, RenderedPage, SearchMatch};

// ── Tab system types ──────────────────────────────────────────────

pub type TabId = u64;

/// Heavy render data — cleared when a tab is suspended to free memory.
#[derive(Debug, Clone, Default)]
pub struct PageRenderData {
    pub rendered_pages: Vec<RenderedPageData>,
    pub text_data: HashMap<u32, PageTextData>,
    pub thumbnails: Vec<RenderedPageData>,
    pub _page_dimensions: Vec<(f32, f32)>,
}

/// Zoom and scroll state for a document.
#[derive(Debug, Clone)]
pub struct ViewState {
    pub zoom: f32,
    pub render_zoom: f32,
    pub scroll_top: f64,
    pub page_batch_size: u32,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.5,
            render_zoom: 1.5,
            scroll_top: 0.0,
            page_batch_size: 5,
        }
    }
}

/// In-document text search state.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub visible: bool,
    pub query: String,
    pub matches: Vec<SearchMatch>,
    pub current_index: usize,
}

/// Navigation panel state (per-tab).
#[derive(Debug, Clone, Default)]
pub struct NavPanels {
    pub show_thumbnails: bool,
    pub show_outline: bool,
    pub outline: Vec<BookmarkEntry>,
}

/// One open PDF tab.
#[derive(Debug, Clone)]
pub struct PdfTab {
    pub id: TabId,
    pub pdf_path: String,
    pub paper_id: Option<i64>,
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
    pub fn new(id: TabId, pdf_path: String, title: String, zoom: f32, batch_size: u32) -> Self {
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
                render_zoom: zoom,
                page_batch_size: batch_size,
                ..Default::default()
            },
            search: SearchState::default(),
            nav: NavPanels::default(),
            annotations: Vec::new(),
        }
    }

    /// Whether this tab needs re-rendering (was suspended and has no pages).
    pub fn needs_render(&self) -> bool {
        self.render.rendered_pages.is_empty() && self.page_count > 0
    }

    /// Clear heavy render data to free memory (called on suspend).
    pub fn suspend(&mut self) {
        self.is_suspended = true;
        self.render = PageRenderData::default();
    }
}

/// Manages all open PDF tabs.
#[derive(Debug, Clone, Default)]
pub struct PdfTabManager {
    pub tabs: Vec<PdfTab>,
    pub active_tab_id: Option<TabId>,
    next_id: u64,
}

impl PdfTabManager {
    pub fn next_id(&mut self) -> TabId {
        self.next_id += 1;
        self.next_id
    }

    pub fn find_by_paper_id(&self, paper_id: i64) -> Option<usize> {
        self.tabs.iter().position(|t| t.paper_id == Some(paper_id))
    }

    pub fn find_by_path(&self, path: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.pdf_path == path)
    }

    pub fn active_tab(&self) -> Option<&PdfTab> {
        self.active_tab_id.and_then(|id| self.tabs.iter().find(|t| t.id == id))
    }

    pub fn active_tab_mut(&mut self) -> Option<&mut PdfTab> {
        self.active_tab_id.and_then(|id| self.tabs.iter_mut().find(|t| t.id == id))
    }

    /// Convenience: get active tab, panics if none. Use only in components
    /// that are conditionally rendered when a tab exists.
    pub fn tab(&self) -> &PdfTab {
        self.active_tab().expect("no active tab")
    }

    pub fn tab_mut(&mut self) -> &mut PdfTab {
        self.active_tab_mut().expect("no active tab")
    }

    /// Switch to a tab, suspending the previous one.
    pub fn switch_to(&mut self, tab_id: TabId) {
        if let Some(old_id) = self.active_tab_id {
            if old_id != tab_id {
                if let Some(old_tab) = self.tabs.iter_mut().find(|t| t.id == old_id) {
                    old_tab.suspend();
                }
            }
        }
        self.active_tab_id = Some(tab_id);
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.is_suspended = false;
        }
    }

    /// Close a tab. Returns true if the active tab changed.
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

    /// Close all tabs except the given one.
    pub fn close_others(&mut self, keep_id: TabId) {
        self.tabs.retain(|t| t.id == keep_id);
        self.active_tab_id = Some(keep_id);
    }

    /// Close all tabs to the right of the given one.
    pub fn close_to_right(&mut self, tab_id: TabId) {
        if let Some(idx) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.truncate(idx + 1);
            // If active tab was removed, switch to the kept tab
            if let Some(active) = self.active_tab_id {
                if !self.tabs.iter().any(|t| t.id == active) {
                    self.active_tab_id = Some(tab_id);
                }
            }
        }
    }

    /// Add a new tab and make it active.
    pub fn open_tab(&mut self, tab: PdfTab) -> TabId {
        let tab_id = tab.id;
        self.tabs.push(tab);
        self.switch_to(tab_id);
        tab_id
    }
}

/// Shared viewer tool state (not per-tab).
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

// ── Shared types (unchanged) ──────────────────────────────────────

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

/// Lightweight version of RenderedPage for storage in signals.
/// Uses `Arc<String>` for the base64 data so that cloning page lists
/// (which happens every Dioxus render cycle) is near-free instead of
/// copying hundreds of KB of base64 per page.
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

// ── Library state (unchanged) ─────────────────────────────────────

/// Tracks the library state: papers, collections, tags.
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub papers: Vec<Paper>,
    pub collections: Vec<Collection>,
    pub tags: Vec<Tag>,
    pub selected_paper_id: Option<i64>,
    pub _selected_collection_id: Option<i64>,
    pub view: LibraryView,
    pub search_query: String,
    pub search_results: Option<Vec<Paper>>,
    pub collection_paper_ids: Option<Vec<i64>>,
    pub tag_paper_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LibraryView {
    #[default]
    AllPapers,
    RecentlyAdded,
    Favorites,
    Unread,
    Collection(i64),
    Tag(i64),
    PdfViewer,
}

impl LibraryState {
    pub fn selected_paper(&self) -> Option<&Paper> {
        self.selected_paper_id.and_then(|id| {
            self.papers.iter().find(|p| p.id == Some(id))
        })
    }
}

/// Newtype for drag-paper signal to avoid context ambiguity with other `Signal<Option<i64>>`.
#[derive(Debug, Clone, Copy)]
pub struct DragPaper(pub Option<i64>);

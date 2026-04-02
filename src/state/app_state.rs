use rotero_models::{Collection, Paper, Tag};
use rotero_pdf::RenderedPage;

/// Tracks which PDF is currently open and its rendered pages.
#[derive(Debug, Clone, Default)]
pub struct PdfViewState {
    pub pdf_path: Option<String>,
    pub page_count: u32,
    pub current_page: u32,
    pub zoom: f32,
    pub rendered_pages: Vec<RenderedPageData>,
}

/// Lightweight version of RenderedPage for storage in signals.
#[derive(Debug, Clone)]
pub struct RenderedPageData {
    pub page_index: u32,
    pub base64_png: String,
    pub width: u32,
    pub height: u32,
}

impl From<RenderedPage> for RenderedPageData {
    fn from(rp: RenderedPage) -> Self {
        Self {
            page_index: rp.page_index,
            base64_png: rp.base64_png,
            width: rp.width,
            height: rp.height,
        }
    }
}

impl PdfViewState {
    pub fn new() -> Self {
        Self {
            zoom: 1.5,
            ..Default::default()
        }
    }
}

/// Tracks the library state: papers, collections, tags.
#[derive(Debug, Clone, Default)]
pub struct LibraryState {
    pub papers: Vec<Paper>,
    pub collections: Vec<Collection>,
    pub tags: Vec<Tag>,
    pub selected_paper_id: Option<i64>,
    pub selected_collection_id: Option<i64>,
    pub view: LibraryView,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum LibraryView {
    #[default]
    AllPapers,
    Collection(i64),
    PdfViewer,
}

impl LibraryState {
    pub fn selected_paper(&self) -> Option<&Paper> {
        self.selected_paper_id.and_then(|id| {
            self.papers.iter().find(|p| p.id == Some(id))
        })
    }
}

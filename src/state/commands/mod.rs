mod citations;
mod import;
mod library_helpers;
mod pdf_cache;
mod pdf_extract;
mod pdf_loading;

pub use citations::*;
pub use import::*;
#[cfg(test)]
mod import_queue_test;
pub use library_helpers::*;
pub use pdf_cache::*;
pub use pdf_extract::*;
pub use pdf_loading::*;

use std::collections::HashMap;
use std::sync::Arc;

use rotero_pdf::PageTextData;
use rotero_pdf::RenderSession;

use super::app_state::RenderedPageData;

pub type PdfExtractResult = (Vec<(u32, String)>, rotero_pdf::PdfDocMetadata);

/// Shared PDF document cache + optional concurrency limit for blocking work.
///
/// `pdfrum::Document` is `Send + Sync`, so call sites `spawn_blocking` against this
/// handle and get a plain `Result` — no mpsc request enum or oneshot replies.
#[derive(Clone)]
pub struct PdfDocs {
    cache: Arc<rotero_pdf::DocCache>,
    sem: Arc<tokio::sync::Semaphore>,
}

impl PdfDocs {
    /// Creates a handle with a fresh [`DocCache`] and a semaphore sized to
    /// available parallelism (clamped 2..=8).
    pub fn new() -> Self {
        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 8))
            .unwrap_or(4);
        Self {
            cache: Arc::new(rotero_pdf::DocCache::new()),
            sem: Arc::new(tokio::sync::Semaphore::new(concurrency)),
        }
    }

    /// Drops all cached `Arc<Document>`s. Call when the last PDF tab closes.
    pub fn clear(&self) {
        self.cache.clear();
    }

    async fn run<F, T>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(&rotero_pdf::DocCache) -> Result<T, String> + Send + 'static,
        T: Send + 'static,
    {
        let _permit = self
            .sem
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let cache = Arc::clone(&self.cache);
        tokio::task::spawn_blocking(move || f(&cache))
            .await
            .map_err(|e| e.to_string())?
    }

    /// Opens via cache and renders the first `batch_size` pages.
    pub async fn open_and_render_initial(
        &self,
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
    ) -> Result<(u32, Vec<RenderedPageData>), String> {
        self.open_and_render_initial_with(pdf_path, zoom, batch_size, None)
            .await
    }

    /// Open (with optional password) and render the first batch of pages.
    ///
    /// Maps [`rotero_pdf::PdfError::WrongPassword`] to the sentinel string
    /// `"password-required"` so the UI can show an unlock prompt.
    pub async fn open_and_render_initial_with(
        &self,
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
        password: Option<String>,
    ) -> Result<(u32, Vec<RenderedPageData>), String> {
        self.open_and_render_initial_with_theme(pdf_path, zoom, batch_size, password, false)
            .await
    }

    /// Open and render with optional dark / night-mode colour scheme.
    pub async fn open_and_render_initial_with_theme(
        &self,
        pdf_path: String,
        zoom: f32,
        batch_size: u32,
        password: Option<String>,
        dark: bool,
    ) -> Result<(u32, Vec<RenderedPageData>), String> {
        self.run(move |cache| {
            let (page_count, rendered) = rotero_pdf::open_and_render_initial_with_theme(
                cache,
                &pdf_path,
                zoom,
                batch_size,
                password.as_deref(),
                dark,
            )
            .map_err(|e| match e {
                rotero_pdf::PdfError::WrongPassword => "password-required".to_string(),
                other => other.to_string(),
            })?;
            let pages: Vec<RenderedPageData> = rendered.into_iter().map(|r| r.into()).collect();
            Ok((page_count, pages))
        })
        .await
    }

    /// Renders a contiguous page range at `zoom`.
    pub async fn render_pages(
        &self,
        pdf_path: String,
        start: u32,
        count: u32,
        zoom: f32,
    ) -> Result<Vec<RenderedPageData>, String> {
        self.render_pages_with(pdf_path, start, count, zoom, false)
            .await
    }

    /// Render pages with optional dark / night-mode colour scheme.
    pub async fn render_pages_with(
        &self,
        pdf_path: String,
        start: u32,
        count: u32,
        zoom: f32,
        dark: bool,
    ) -> Result<Vec<RenderedPageData>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            let mut session = RenderSession::new();
            let rendered =
                rotero_pdf::render_pages_with(&doc, start, count, zoom, dark, &mut session)
                    .map_err(|e| e.to_string())?;
            Ok(rendered.into_iter().map(|r| r.into()).collect())
        })
        .await
    }

    /// Renders thumbnails (max width 120px) for a page range.
    pub async fn render_thumbnails(
        &self,
        pdf_path: String,
        start: u32,
        count: u32,
    ) -> Result<Vec<RenderedPageData>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            let mut session = RenderSession::new();
            let rendered = rotero_pdf::render_thumbnails(&doc, start, count, 120, &mut session)
                .map_err(|e| e.to_string())?;
            Ok(rendered.into_iter().map(|r| r.into()).collect())
        })
        .await
    }

    /// Extracts positioned text for the given page pixel dimensions.
    pub async fn extract_text(
        &self,
        pdf_path: String,
        page_dims: Vec<(u32, u32, u32)>,
    ) -> Result<HashMap<u32, PageTextData>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            let text_pages = rotero_pdf::text_extract::extract_pages_text(&doc, &page_dims)
                .map_err(|e| e.to_string())?;
            Ok(text_pages.into_iter().map(|t| (t.page_index, t)).collect())
        })
        .await
    }

    /// Extracts the document outline (bookmarks).
    pub async fn extract_outline(
        &self,
        pdf_path: String,
    ) -> Result<Vec<rotero_pdf::BookmarkEntry>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            Ok(rotero_pdf::outline(&doc))
        })
        .await
    }

    /// `/PageLabels` for every page (`None` = ordinal numbering).
    pub async fn page_labels(&self, pdf_path: String) -> Result<Vec<Option<String>>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            Ok(rotero_pdf::page_labels(&doc))
        })
        .await
    }

    /// Markdown for one page (pdfrum structure/typography extract).
    pub async fn page_markdown(&self, pdf_path: String, page_index: u32) -> Result<String, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            rotero_pdf::page_markdown(&doc, page_index).map_err(|e| e.to_string())
        })
        .await
    }

    /// Whole-document Markdown.
    #[allow(dead_code)] // used by MCP / future callers
    pub async fn document_markdown(&self, pdf_path: String) -> Result<String, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            Ok(rotero_pdf::document_markdown(&doc))
        })
        .await
    }

    /// List embedded figures on a page (metadata only).
    pub async fn list_page_images(
        &self,
        pdf_path: String,
        page_index: u32,
    ) -> Result<Vec<rotero_pdf::PageFigure>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            rotero_pdf::list_page_images(&doc, page_index).map_err(|e| e.to_string())
        })
        .await
    }

    /// Extract one page figure as PNG (base64).
    pub async fn extract_page_image_png(
        &self,
        pdf_path: String,
        page_index: u32,
        image_index: u32,
    ) -> Result<rotero_pdf::PageFigure, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            rotero_pdf::extract_page_image_png(&doc, page_index, image_index)
                .map_err(|e| e.to_string())
        })
        .await
    }

    /// Returns (width_pts, height_pts) for every page.
    pub async fn page_dimensions(&self, pdf_path: String) -> Result<Vec<(f32, f32)>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            Ok(rotero_pdf::page_dimensions(&doc))
        })
        .await
    }

    /// Raw text + PDF info-dict metadata for the first `page_count` pages.
    pub async fn extract_metadata_text(
        &self,
        pdf_path: String,
        page_count: u32,
    ) -> Result<PdfExtractResult, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            let indices: Vec<u32> = (0..page_count).collect();
            let raw_text = rotero_pdf::text_extract::extract_raw_text(&doc, &indices)
                .map_err(|e| e.to_string())?;
            let doc_meta = rotero_pdf::text_extract::extract_doc_metadata(&doc);
            Ok((raw_text, doc_meta))
        })
        .await
    }

    /// Full-text search via pdfrum `TextPage::find_with`, returning pixel-space matches.
    pub async fn search(
        &self,
        pdf_path: String,
        query: String,
        page_pixel_dims: Vec<(u32, u32)>,
    ) -> Result<Vec<rotero_pdf::SearchMatch>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            Ok(rotero_pdf::search_in_document(
                &doc,
                &query,
                &page_pixel_dims,
            ))
        })
        .await
    }

    /// Char-level Highlight/Underline markup for a pixel-space drag rect.
    ///
    /// Uses the cached document (already open while a tab is viewing the PDF)
    /// so the overlay can call this synchronously during drag preview / mouseup.
    #[allow(clippy::too_many_arguments)] // path + page + pixel size + selection AABB
    pub fn selection_markup(
        &self,
        pdf_path: &str,
        page_index: u32,
        img_width: u32,
        img_height: u32,
        sel_x: f64,
        sel_y: f64,
        sel_w: f64,
        sel_h: f64,
    ) -> Option<rotero_pdf::SelectionMarkup> {
        let doc = self.cache.open(pdf_path).ok()?;
        rotero_pdf::selection_markup(
            &doc, page_index, img_width, img_height, sel_x, sel_y, sel_w, sel_h,
        )
    }

    /// Word or line selection under a pixel click (double / triple click).
    #[allow(clippy::too_many_arguments)]
    pub fn selection_at_point(
        &self,
        pdf_path: &str,
        page_index: u32,
        img_width: u32,
        img_height: u32,
        pixel_x: f64,
        pixel_y: f64,
        mode: rotero_pdf::ClickSelectMode,
    ) -> Option<rotero_pdf::SelectionMarkup> {
        let doc = self.cache.open(pdf_path).ok()?;
        rotero_pdf::selection_at_point(
            &doc, page_index, img_width, img_height, pixel_x, pixel_y, mode,
        )
    }

    /// Extracts supported annotations from the PDF.
    pub async fn extract_annotations(
        &self,
        pdf_path: String,
    ) -> Result<Vec<rotero_pdf::ExtractedAnnotation>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            Ok(rotero_pdf::extract_annotations(&doc))
        })
        .await
    }

    /// Extracts intra-document and external links.
    pub async fn extract_links(
        &self,
        pdf_path: String,
    ) -> Result<Vec<rotero_pdf::ExtractedLink>, String> {
        self.run(move |cache| {
            let doc = cache.open(&pdf_path).map_err(|e| e.to_string())?;
            rotero_pdf::extract_links(&doc).map_err(|e| e.to_string())
        })
        .await
    }
}

impl Default for PdfDocs {
    fn default() -> Self {
        Self::new()
    }
}

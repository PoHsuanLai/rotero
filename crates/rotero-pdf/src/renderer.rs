use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::codecs::jpeg::JpegEncoder;
use pdfium_render::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PdfError {
    #[error("Failed to bind PDFium library: {0}")]
    BindError(String),
    #[error("Failed to load PDF: {0}")]
    LoadError(#[from] PdfiumError),
    #[error("Page {0} out of range (total: {1})")]
    PageOutOfRange(u32, u32),
    #[error("Failed to render page: {0}")]
    RenderError(String),
    #[error("Failed to encode image: {0}")]
    ImageError(#[from] image::ImageError),
    #[error("Failed to write annotations: {0}")]
    WriteError(String),
}

/// Holds the PDFium bindings and provides PDF operations.
/// Each operation loads the document fresh to avoid borrow issues with pdfium-render's
/// lifetime-bound PdfDocument type.
pub struct PdfEngine {
    pdfium: Pdfium,
}

pub struct PdfDocumentInfo {
    pub path: String,
    pub page_count: u32,
}

pub struct RenderedPage {
    pub page_index: u32,
    pub base64_data: String,
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

impl PdfEngine {
    /// Access the underlying Pdfium bindings (for text extraction, etc.)
    pub fn pdfium(&self) -> &Pdfium {
        &self.pdfium
    }

    /// Create a new PdfEngine by binding to the PDFium library.
    ///
    /// Resolution order:
    /// 1. Explicit `lib_path` argument
    /// 2. `PDFIUM_DYNAMIC_LIB_PATH` env var (directory containing the library)
    /// 3. System library search paths
    pub fn new(lib_path: Option<&str>) -> Result<Self, PdfError> {
        let bindings = if let Some(path) = lib_path {
            Pdfium::bind_to_library(path)
                .map_err(|e| PdfError::BindError(e.to_string()))?
        } else if let Ok(dir) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
            let lib_name = Pdfium::pdfium_platform_library_name_at_path(&dir);
            Pdfium::bind_to_library(lib_name)
                .map_err(|e| PdfError::BindError(format!("PDFIUM_DYNAMIC_LIB_PATH={dir}: {e}")))?
        } else {
            Pdfium::bind_to_system_library()
                .map_err(|e| PdfError::BindError(e.to_string()))?
        };

        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }

    /// Load a PDF and return basic info (page count, path).
    pub fn load_document(&self, pdf_path: &str) -> Result<PdfDocumentInfo, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        Ok(PdfDocumentInfo {
            path: pdf_path.to_string(),
            page_count: document.pages().len() as u32,
        })
    }

    /// Render a single page to a base64-encoded JPEG string.
    /// `scale` controls the zoom level (1.0 = 72 DPI, 2.0 = 144 DPI, etc.)
    /// `quality` is JPEG quality (1-100).
    pub fn render_page(
        &self,
        pdf_path: &str,
        page_index: u32,
        scale: f32,
        quality: u8,
    ) -> Result<RenderedPage, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        let page_count = document.pages().len() as u32;

        if page_index >= page_count {
            return Err(PdfError::PageOutOfRange(page_index, page_count));
        }

        let page = document
            .pages()
            .get(page_index as u16)
            .map_err(|e| PdfError::RenderError(e.to_string()))?;

        let width = (page.width().value * scale) as i32;
        let height = (page.height().value * scale) as i32;

        let render_config = PdfRenderConfig::new()
            .set_target_width(width.max(1))
            .set_maximum_height(height.max(1));

        let bitmap = page
            .render_with_config(&render_config)
            .map_err(|e| PdfError::RenderError(e.to_string()))?;

        let image = bitmap.as_image();
        let img_width = image.width();
        let img_height = image.height();

        let mut img_bytes: Vec<u8> = Vec::new();
        let encoder = JpegEncoder::new_with_quality(&mut img_bytes, quality);
        image.write_with_encoder(encoder)?;

        let base64_data = BASE64.encode(&img_bytes);

        Ok(RenderedPage {
            page_index,
            base64_data,
            mime: "image/jpeg",
            width: img_width,
            height: img_height,
        })
    }

    /// Render multiple pages (useful for pre-rendering visible pages).
    pub fn render_pages(
        &self,
        pdf_path: &str,
        start: u32,
        count: u32,
        scale: f32,
        quality: u8,
    ) -> Result<Vec<RenderedPage>, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        let page_count = document.pages().len() as u32;
        let end = (start + count).min(page_count);

        let mut pages = Vec::new();
        for i in start..end {
            let t_page = std::time::Instant::now();
            let page = document
                .pages()
                .get(i as u16)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;

            let width = (page.width().value * scale) as i32;
            let height = (page.height().value * scale) as i32;

            let render_config = PdfRenderConfig::new()
                .set_target_width(width.max(1))
                .set_maximum_height(height.max(1));

            let t_render = std::time::Instant::now();
            let bitmap = page
                .render_with_config(&render_config)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;
            let render_ms = t_render.elapsed();

            let t_encode = std::time::Instant::now();
            let image = bitmap.as_image();
            let img_width = image.width();
            let img_height = image.height();

            let mut img_bytes: Vec<u8> = Vec::new();
            let encoder = JpegEncoder::new_with_quality(&mut img_bytes, quality);
            image.write_with_encoder(encoder)?;
            let encode_ms = t_encode.elapsed();

            let base64_data = BASE64.encode(&img_bytes);

            tracing::info!(
                page = i, width, height,
                render_ms = ?render_ms, encode_ms = ?encode_ms,
                jpeg_kb = img_bytes.len() / 1024,
                base64_kb = base64_data.len() / 1024,
                total_ms = ?t_page.elapsed(),
                "rendered page"
            );

            pages.push(RenderedPage {
                page_index: i,
                base64_data,
                mime: "image/jpeg",
                width: img_width,
                height: img_height,
            });
        }

        Ok(pages)
    }

    /// Render all pages as small thumbnails (fixed max width).
    pub fn render_all_thumbnails(
        &self,
        pdf_path: &str,
        max_width: u32,
        quality: u8,
    ) -> Result<Vec<RenderedPage>, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        let page_count = document.pages().len() as u32;
        let mut thumbs = Vec::new();

        for i in 0..page_count {
            let page = document
                .pages()
                .get(i as u16)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;

            let aspect = page.height().value / page.width().value;
            let target_width = max_width as i32;
            let target_height = (max_width as f32 * aspect) as i32;

            let render_config = PdfRenderConfig::new()
                .set_target_width(target_width.max(1))
                .set_maximum_height(target_height.max(1));

            let bitmap = page
                .render_with_config(&render_config)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;

            let image = bitmap.as_image();
            let img_width = image.width();
            let img_height = image.height();

            let mut img_bytes: Vec<u8> = Vec::new();
            let encoder = JpegEncoder::new_with_quality(&mut img_bytes, quality);
            image.write_with_encoder(encoder)?;

            let base64_data = BASE64.encode(&img_bytes);

            thumbs.push(RenderedPage {
                page_index: i,
                base64_data,
                mime: "image/jpeg",
                width: img_width,
                height: img_height,
            });
        }

        Ok(thumbs)
    }

    /// Extract the document outline/bookmarks.
    pub fn extract_outline(
        &self,
        pdf_path: &str,
    ) -> Result<Vec<BookmarkEntry>, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        let bookmarks = document.bookmarks();
        let mut entries = Vec::new();

        fn collect_bookmarks(
            iter: &pdfium_render::prelude::PdfBookmarks,
            entries: &mut Vec<BookmarkEntry>,
            level: u32,
        ) {
            for bookmark in iter.iter() {
                let title = bookmark.title().unwrap_or_default();
                let page_index = bookmark.destination()
                    .and_then(|d| d.page_index().ok())
                    .map(|i| i as u32);

                entries.push(BookmarkEntry {
                    title,
                    page_index,
                    level,
                });
            }
        }

        collect_bookmarks(&bookmarks, &mut entries, 0);
        Ok(entries)
    }

    /// Get page dimensions (in points) for all pages without rendering.
    pub fn get_page_dimensions(
        &self,
        pdf_path: &str,
    ) -> Result<Vec<(f32, f32)>, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        let page_count = document.pages().len() as u32;
        let mut dims = Vec::new();

        for i in 0..page_count {
            let page = document
                .pages()
                .get(i as u16)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;
            dims.push((page.width().value, page.height().value));
        }

        Ok(dims)
    }
}

/// A bookmark/outline entry from the PDF.
#[derive(Debug, Clone)]
pub struct BookmarkEntry {
    pub title: String,
    pub page_index: Option<u32>,
    pub level: u32,
}

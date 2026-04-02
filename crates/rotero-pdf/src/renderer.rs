use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::ImageFormat;
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
    pub base64_png: String,
    pub width: u32,
    pub height: u32,
}

impl PdfEngine {
    /// Create a new PdfEngine by binding to the PDFium library.
    pub fn new(lib_path: Option<&str>) -> Result<Self, PdfError> {
        let bindings = if let Some(path) = lib_path {
            Pdfium::bind_to_library(path)
        } else {
            Pdfium::bind_to_system_library()
        }
        .map_err(|e| PdfError::BindError(e.to_string()))?;

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

    /// Render a single page to a base64-encoded PNG string.
    /// `scale` controls the zoom level (1.0 = 72 DPI, 2.0 = 144 DPI, etc.)
    pub fn render_page(
        &self,
        pdf_path: &str,
        page_index: u32,
        scale: f32,
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

        let mut png_bytes: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut png_bytes);
        image.write_to(&mut cursor, ImageFormat::Png)?;

        let base64_png = BASE64.encode(&png_bytes);

        Ok(RenderedPage {
            page_index,
            base64_png,
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
    ) -> Result<Vec<RenderedPage>, PdfError> {
        let document = self.pdfium.load_pdf_from_file(pdf_path, None)?;
        let page_count = document.pages().len() as u32;
        let end = (start + count).min(page_count);

        let mut pages = Vec::new();
        for i in start..end {
            let page = document
                .pages()
                .get(i as u16)
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

            let mut png_bytes: Vec<u8> = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut png_bytes);
            image.write_to(&mut cursor, ImageFormat::Png)?;

            let base64_png = BASE64.encode(&png_bytes);

            pages.push(RenderedPage {
                page_index: i,
                base64_png,
                width: img_width,
                height: img_height,
            });
        }

        Ok(pages)
    }
}

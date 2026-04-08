use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use image::codecs::png::PngEncoder;
use pdfium_render::prelude::*;
use thiserror::Error;

fn encode_png(image: &image::DynamicImage, buf: &mut Vec<u8>) -> Result<(), image::ImageError> {
    buf.clear();
    let encoder = PngEncoder::new(buf);
    image.write_with_encoder(encoder)
}

fn file_mtime(path: &str) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Errors that can occur during PDF operations (loading, rendering, encoding, annotation writing).
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

/// Caches the most recently used PDF file bytes to avoid redundant disk I/O
/// across repeated operations on the same document.
pub struct PdfEngine {
    pdfium: Pdfium,
    cached_bytes: Option<(String, u64, Vec<u8>)>,
}

/// Basic information about a loaded PDF document.
pub struct PdfDocumentInfo {
    /// Filesystem path of the loaded PDF.
    pub path: String,
    /// Total number of pages in the document.
    pub page_count: u32,
}

/// A single rendered PDF page as a base64-encoded PNG image.
pub struct RenderedPage {
    /// Zero-based page number.
    pub page_index: u32,
    /// Base64-encoded PNG image data.
    pub base64_data: String,
    /// MIME type of the encoded image (always `"image/png"`).
    pub mime: &'static str,
    /// Rendered image width in pixels.
    pub width: u32,
    /// Rendered image height in pixels.
    pub height: u32,
}

impl PdfEngine {
    /// Returns a reference to the underlying pdfium instance.
    pub fn pdfium(&self) -> &Pdfium {
        &self.pdfium
    }

    /// Creates a new engine by binding to a statically linked PDFium library.
    #[cfg(feature = "static")]
    pub fn new_static() -> Result<Self, PdfError> {
        let bindings = Pdfium::bind_to_statically_linked_library()
            .map_err(|e| PdfError::BindError(e.to_string()))?;
        Ok(Self {
            pdfium: Pdfium::new(bindings),
            cached_bytes: None,
        })
    }

    /// Resolution order: explicit `lib_path`, then `PDFIUM_DYNAMIC_LIB_PATH` env var, then system paths.
    #[cfg(not(feature = "static"))]
    pub fn new(lib_path: Option<&str>) -> Result<Self, PdfError> {
        let bindings = if let Some(path) = lib_path {
            Pdfium::bind_to_library(path).map_err(|e| PdfError::BindError(e.to_string()))?
        } else if let Ok(dir) = std::env::var("PDFIUM_DYNAMIC_LIB_PATH") {
            let lib_name = Pdfium::pdfium_platform_library_name_at_path(&dir);
            Pdfium::bind_to_library(lib_name)
                .map_err(|e| PdfError::BindError(format!("PDFIUM_DYNAMIC_LIB_PATH={dir}: {e}")))?
        } else {
            Pdfium::bind_to_system_library().map_err(|e| PdfError::BindError(e.to_string()))?
        };

        Ok(Self {
            pdfium: Pdfium::new(bindings),
            cached_bytes: None,
        })
    }

    fn get_pdf_bytes(&mut self, pdf_path: &str) -> Result<Vec<u8>, PdfError> {
        let mtime = file_mtime(pdf_path);
        if let Some((ref cached_path, cached_mtime, ref bytes)) = self.cached_bytes
            && cached_path == pdf_path
            && cached_mtime == mtime
        {
            return Ok(bytes.clone());
        }
        let bytes = std::fs::read(pdf_path)
            .map_err(|e| PdfError::RenderError(format!("Failed to read {pdf_path}: {e}")))?;
        self.cached_bytes = Some((pdf_path.to_string(), mtime, bytes));
        Ok(self.cached_bytes.as_ref().unwrap().2.clone())
    }

    fn open_document(
        &mut self,
        pdf_path: &str,
    ) -> Result<pdfium_render::prelude::PdfDocument<'_>, PdfError> {
        let bytes = self.get_pdf_bytes(pdf_path)?;
        Ok(self.pdfium.load_pdf_from_byte_vec(bytes, None)?)
    }

    /// Loads a PDF and returns its path and page count without rendering.
    pub fn load_document(&mut self, pdf_path: &str) -> Result<PdfDocumentInfo, PdfError> {
        let document = self.open_document(pdf_path)?;
        Ok(PdfDocumentInfo {
            path: pdf_path.to_string(),
            page_count: document.pages().len() as u32,
        })
    }

    /// `scale`: zoom level (1.0 = 72 DPI, 2.0 = 144 DPI, etc.)
    pub fn render_page(
        &mut self,
        pdf_path: &str,
        page_index: u32,
        scale: f32,
    ) -> Result<RenderedPage, PdfError> {
        let document = self.open_document(pdf_path)?;
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

        let mut img_bytes: Vec<u8> = Vec::with_capacity(256 * 1024);
        encode_png(&image, &mut img_bytes)?;

        let base64_data = BASE64.encode(&img_bytes);

        Ok(RenderedPage {
            page_index,
            base64_data,
            mime: "image/png",
            width: img_width,
            height: img_height,
        })
    }

    /// Renders a contiguous range of pages starting at `start` as base64 PNGs.
    pub fn render_pages(
        &mut self,
        pdf_path: &str,
        start: u32,
        count: u32,
        scale: f32,
    ) -> Result<Vec<RenderedPage>, PdfError> {
        let document = self.open_document(pdf_path)?;
        let page_count = document.pages().len() as u32;
        let end = (start + count).min(page_count);

        let mut pages = Vec::with_capacity((end - start) as usize);
        let mut img_bytes: Vec<u8> = Vec::with_capacity(256 * 1024);
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

            encode_png(&image, &mut img_bytes)?;

            let base64_data = BASE64.encode(&img_bytes);

            pages.push(RenderedPage {
                page_index: i,
                base64_data,
                mime: "image/png",
                width: img_width,
                height: img_height,
            });
        }

        Ok(pages)
    }

    /// Renders a range of pages as thumbnails constrained to `max_width` pixels.
    pub fn render_thumbnails_range(
        &mut self,
        pdf_path: &str,
        start: u32,
        count: u32,
        max_width: u32,
    ) -> Result<Vec<RenderedPage>, PdfError> {
        let document = self.open_document(pdf_path)?;
        let page_count = document.pages().len() as u32;
        let end = (start + count).min(page_count);
        let mut thumbs = Vec::with_capacity((end - start) as usize);
        let mut img_bytes: Vec<u8> = Vec::with_capacity(64 * 1024);

        for i in start..end {
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

            encode_png(&image, &mut img_bytes)?;

            let base64_data = BASE64.encode(&img_bytes);

            thumbs.push(RenderedPage {
                page_index: i,
                base64_data,
                mime: "image/png",
                width: img_width,
                height: img_height,
            });
        }

        Ok(thumbs)
    }

    /// Extracts the document outline (bookmarks / table of contents).
    pub fn extract_outline(&mut self, pdf_path: &str) -> Result<Vec<BookmarkEntry>, PdfError> {
        let document = self.open_document(pdf_path)?;
        let bookmarks = document.bookmarks();
        let mut entries = Vec::new();

        fn collect_bookmarks(
            iter: &pdfium_render::prelude::PdfBookmarks,
            entries: &mut Vec<BookmarkEntry>,
            level: u32,
        ) {
            for bookmark in iter.iter() {
                let title = bookmark.title().unwrap_or_default();
                let page_index = bookmark
                    .destination()
                    .and_then(|d| d.page_index().ok())
                    .map(|i| i as u32);

                entries.push(BookmarkEntry {
                    title,
                    page_index,
                    level,
                });
            }
        }

        collect_bookmarks(bookmarks, &mut entries, 0);
        Ok(entries)
    }

    /// Returns (width_pts, height_pts) for all pages without rendering.
    pub fn get_page_dimensions(&mut self, pdf_path: &str) -> Result<Vec<(f32, f32)>, PdfError> {
        let document = self.open_document(pdf_path)?;
        let page_count = document.pages().len() as u32;
        let mut dims = Vec::with_capacity(page_count as usize);

        for i in 0..page_count {
            let page = document
                .pages()
                .get(i as u16)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;
            dims.push((page.width().value, page.height().value));
        }

        Ok(dims)
    }

    /// Drops the cached PDF file bytes to free memory.
    pub fn clear_byte_cache(&mut self) {
        self.cached_bytes = None;
    }

    /// Reads all supported annotations (highlights, notes, areas, underlines, ink, free text) from the PDF.
    pub fn extract_annotations(
        &mut self,
        pdf_path: &str,
    ) -> Result<Vec<ExtractedAnnotation>, PdfError> {
        let document = self.open_document(pdf_path)?;
        let page_count = document.pages().len() as u32;
        let mut result = Vec::new();

        for i in 0..page_count {
            let page = document
                .pages()
                .get(i as u16)
                .map_err(|e| PdfError::RenderError(e.to_string()))?;

            let pw = page.width().value;
            let ph = page.height().value;

            for ann in page.annotations().iter() {
                use pdfium_render::prelude::PdfPageAnnotationCommon;

                let ann_type = match ann.annotation_type() {
                    PdfPageAnnotationType::Highlight => rotero_models::AnnotationType::Highlight,
                    PdfPageAnnotationType::Text => rotero_models::AnnotationType::Note,
                    PdfPageAnnotationType::Square => rotero_models::AnnotationType::Area,
                    PdfPageAnnotationType::Underline => rotero_models::AnnotationType::Underline,
                    PdfPageAnnotationType::Ink => rotero_models::AnnotationType::Ink,
                    PdfPageAnnotationType::FreeText => rotero_models::AnnotationType::Text,
                    _ => continue,
                };

                let bounds = match ann.bounds() {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let color = ann
                    .stroke_color()
                    .or_else(|_| ann.fill_color())
                    .map(|c| format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue()))
                    .unwrap_or_else(|_| "#ffff00".to_string());

                let content = ann.contents();

                result.push(ExtractedAnnotation {
                    page: i,
                    ann_type,
                    color,
                    content,
                    rect_pts: [
                        bounds.left().value,
                        bounds.bottom().value,
                        bounds.right().value,
                        bounds.top().value,
                    ],
                    page_width_pts: pw,
                    page_height_pts: ph,
                });
            }
        }

        Ok(result)
    }
}

/// An annotation extracted from a PDF page, with bounds in PDF point coordinates.
#[derive(Debug, Clone)]
pub struct ExtractedAnnotation {
    /// Zero-based page number where the annotation appears.
    pub page: u32,
    /// The annotation type (highlight, note, area, underline, ink, or free text).
    pub ann_type: rotero_models::AnnotationType,
    /// Hex color string (e.g. `"#ffff00"`).
    pub color: String,
    /// Optional text content or comment attached to the annotation.
    pub content: Option<String>,
    /// [x1 (left), y1 (bottom), x2 (right), y2 (top)] in PDF points.
    pub rect_pts: [f32; 4],
    /// Width of the containing page in PDF points.
    pub page_width_pts: f32,
    /// Height of the containing page in PDF points.
    pub page_height_pts: f32,
}

/// A single entry from the PDF document outline (bookmark tree).
#[derive(Debug, Clone)]
pub struct BookmarkEntry {
    /// Display title of the bookmark.
    pub title: String,
    /// Target page index, if the bookmark has a page destination.
    pub page_index: Option<u32>,
    /// Nesting depth in the outline hierarchy (0 = top level).
    pub level: u32,
}

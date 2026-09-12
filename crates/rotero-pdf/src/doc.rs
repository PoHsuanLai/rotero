//! PDF document cache and operations built on `pdfrum::Document`.
//!
//! `pdfrum::Document` is `Send + Sync`, so callers open via [`DocCache`] (or
//! `Document::open` directly) and call free functions with `&Document`. Each
//! render creates its own [`RenderSession`] (`&mut`-only, not shared).

use std::collections::HashMap;
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use pdfrum::{
    Dest, Document, LinkTarget as PdfrumLinkTarget, Name, Object, RenderOptions, RenderSession,
    Subtype, VelloCpuBackend,
};
use thiserror::Error;

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
    #[error("Failed to load PDF: {0}")]
    LoadError(#[from] pdfrum::Error),
    #[error("Page {0} out of range (total: {1})")]
    PageOutOfRange(u32, u32),
    #[error("Failed to render page: {0}")]
    RenderError(String),
    #[error("Failed to encode image: {0}")]
    ImageError(String),
    #[error("Failed to write annotations: {0}")]
    WriteError(String),
}

struct CachedDoc {
    mtime: u64,
    doc: Arc<Document>,
}

/// Path+mtime keyed cache of open `pdfrum::Document`s.
///
/// Sole responsibility: `open` / `clear` / `evict`. PDF operations live as free
/// functions that take `&Document`.
pub struct DocCache {
    docs: std::sync::Mutex<HashMap<String, CachedDoc>>,
}

impl DocCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self {
            docs: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Opens (or reuses a cached) document for `pdf_path`, keyed by path + mtime.
    pub fn open(&self, pdf_path: &str) -> Result<Arc<Document>, PdfError> {
        let mtime = file_mtime(pdf_path);
        let mut guard = self.docs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = guard.get(pdf_path)
            && cached.mtime == mtime
        {
            return Ok(Arc::clone(&cached.doc));
        }
        let doc = Document::open(pdf_path)?;
        let arc = Arc::new(doc);
        guard.insert(
            pdf_path.to_string(),
            CachedDoc {
                mtime,
                doc: Arc::clone(&arc),
            },
        );
        Ok(arc)
    }

    /// Drops all cached documents so their memory can be reclaimed.
    ///
    /// Callers that already hold an `Arc<Document>` keep it until they drop;
    /// subsequent opens re-read from disk.
    pub fn clear(&self) {
        let mut guard = self.docs.lock().unwrap_or_else(|e| e.into_inner());
        guard.clear();
    }

    /// Evicts a single path from the cache, if present.
    pub fn evict(&self, pdf_path: &str) {
        let mut guard = self.docs.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(pdf_path);
    }
}

impl Default for DocCache {
    fn default() -> Self {
        Self::new()
    }
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

fn encode_rendered(page_index: u32, pixmap: pdfrum::Pixmap) -> Result<RenderedPage, PdfError> {
    let width = pixmap.width();
    let height = pixmap.height();
    let png = pixmap
        .encode_png()
        .map_err(|e| PdfError::ImageError(e.to_string()))?;
    Ok(RenderedPage {
        page_index,
        base64_data: BASE64.encode(&png),
        mime: "image/png",
        width,
        height,
    })
}

fn render_page_inner(
    session: &mut RenderSession,
    doc: &Document,
    page_index: u32,
    opts: &RenderOptions,
) -> Result<RenderedPage, PdfError> {
    let page_count = doc.page_count();
    if page_index >= page_count {
        return Err(PdfError::PageOutOfRange(page_index, page_count));
    }
    let page = doc.page(page_index)?;
    let pixmap = page
        .render_on(VelloCpuBackend, opts, session)
        .map_err(|e| PdfError::RenderError(e.to_string()))?;
    encode_rendered(page_index, pixmap)
}

/// Loads document info (path + page count) without rendering.
pub fn load_document(doc: &Document, pdf_path: &str) -> PdfDocumentInfo {
    PdfDocumentInfo {
        path: pdf_path.to_string(),
        page_count: doc.page_count(),
    }
}

/// Renders a contiguous range of pages starting at `start` as base64 PNGs.
///
/// `scale`: zoom level (1.0 = 72 DPI, 2.0 = 144 DPI, etc.)
pub fn render_pages(
    doc: &Document,
    start: u32,
    count: u32,
    scale: f32,
    session: &mut RenderSession,
) -> Result<Vec<RenderedPage>, PdfError> {
    let page_count = doc.page_count();
    let end = (start + count).min(page_count);
    let opts = RenderOptions::scaled(f64::from(scale));
    let mut pages = Vec::with_capacity((end - start) as usize);
    for i in start..end {
        pages.push(render_page_inner(session, doc, i, &opts)?);
    }
    Ok(pages)
}

/// Opens via [`DocCache`] and renders the first `batch_size` pages.
pub fn open_and_render_initial(
    cache: &DocCache,
    pdf_path: &str,
    scale: f32,
    batch_size: u32,
) -> Result<(u32, Vec<RenderedPage>), PdfError> {
    let doc = cache.open(pdf_path)?;
    let page_count = doc.page_count();
    let mut session = RenderSession::new();
    let pages = render_pages(&doc, 0, batch_size, scale, &mut session)?;
    Ok((page_count, pages))
}

/// Renders a range of pages as thumbnails constrained to `max_width` pixels.
pub fn render_thumbnails(
    doc: &Document,
    start: u32,
    count: u32,
    max_width: u32,
    session: &mut RenderSession,
) -> Result<Vec<RenderedPage>, PdfError> {
    let page_count = doc.page_count();
    let end = (start + count).min(page_count);
    let mut thumbs = Vec::with_capacity((end - start) as usize);

    for i in start..end {
        let page = doc.page(i)?;
        let opts = RenderOptions::fit(page.width(), page.height(), max_width, u32::MAX);
        let pixmap = page
            .render_on(VelloCpuBackend, &opts, session)
            .map_err(|e| PdfError::RenderError(e.to_string()))?;
        thumbs.push(encode_rendered(i, pixmap)?);
    }

    Ok(thumbs)
}

/// Extracts the document outline (bookmarks / table of contents).
pub fn outline(doc: &Document) -> Vec<BookmarkEntry> {
    let outline = doc.outline();
    let mut entries = Vec::with_capacity(outline.len());
    for bookmark in &outline {
        entries.push(BookmarkEntry {
            title: bookmark.title(),
            page_index: bookmark.page_index().map(|i| i.get()),
            level: bookmark.depth() as u32,
        });
    }
    entries
}

/// Returns (width_pts, height_pts) for all pages without rendering.
pub fn page_dimensions(doc: &Document) -> Vec<(f32, f32)> {
    let mut dims = Vec::with_capacity(doc.page_count() as usize);
    for page in doc.pages() {
        dims.push((page.width() as f32, page.height() as f32));
    }
    dims
}

/// Reads all supported annotations from the PDF.
pub fn extract_annotations(doc: &Document) -> Vec<ExtractedAnnotation> {
    let mut result = Vec::new();

    for page in doc.pages() {
        let i = page.index().get();
        let pw = page.width() as f32;
        let ph = page.height() as f32;

        for ann in page.annotations() {
            let ann_type = match ann.subtype() {
                Subtype::Highlight => rotero_models::AnnotationType::Highlight,
                Subtype::Text => rotero_models::AnnotationType::Note,
                Subtype::Square => rotero_models::AnnotationType::Area,
                Subtype::Underline => rotero_models::AnnotationType::Underline,
                Subtype::Ink => rotero_models::AnnotationType::Ink,
                Subtype::FreeText => rotero_models::AnnotationType::Text,
                _ => continue,
            };

            // Prefer /QuadPoints for text-markup subtypes; fall back to /Rect.
            // When multiple quads exist, keep each as rects_pts so import can
            // populate geometry.rects (same shape create/write already uses).
            let (bounds, rects_pts) = match ann_type {
                rotero_models::AnnotationType::Highlight
                | rotero_models::AnnotationType::Underline => {
                    let quads: Vec<_> = ann.quad_points().collect();
                    if quads.is_empty() {
                        (ann.rect(), Vec::new())
                    } else {
                        let mut x0 = f64::INFINITY;
                        let mut y0 = f64::INFINITY;
                        let mut x1 = f64::NEG_INFINITY;
                        let mut y1 = f64::NEG_INFINITY;
                        let mut rects_pts = Vec::with_capacity(quads.len());
                        for q in &quads {
                            let q = q.abs();
                            x0 = x0.min(q.x0);
                            y0 = y0.min(q.y0);
                            x1 = x1.max(q.x1);
                            y1 = y1.max(q.y1);
                            rects_pts.push([q.x0 as f32, q.y0 as f32, q.x1 as f32, q.y1 as f32]);
                        }
                        // Single quad ≡ outer rect; keep rects_pts empty so
                        // import falls back to one geometry box.
                        if rects_pts.len() == 1 {
                            rects_pts.clear();
                        }
                        (pdfrum::Rect::new(x0, y0, x1, y1), rects_pts)
                    }
                }
                _ => (ann.rect(), Vec::new()),
            };
            let color = annot_color_hex(ann.dict());
            let content = ann.contents();

            result.push(ExtractedAnnotation {
                page: i,
                ann_type,
                color,
                content,
                rect_pts: [
                    bounds.x0 as f32,
                    bounds.y0 as f32,
                    bounds.x1 as f32,
                    bounds.y1 as f32,
                ],
                rects_pts,
                page_width_pts: pw,
                page_height_pts: ph,
            });
        }
    }

    result
}

/// Reads all links from the PDF — both intra-document jumps and external URIs.
pub fn extract_links(doc: &Document) -> Result<Vec<ExtractedLink>, PdfError> {
    let mut result = Vec::new();

    let page_heights: Vec<f32> = doc.pages().map(|p| p.height() as f32).collect();

    for page in doc.pages() {
        let i = page.index().get();
        let pw = page.width() as f32;
        let ph = page.height() as f32;
        let resolver = doc.parser();

        // Zip resolved page_links (page/URI) with raw Link dicts (for Dest Y).
        let raw_links = page.links();
        let page_links = page.page_links();

        for (plink, raw) in page_links.into_iter().zip(raw_links) {
            let rect = plink.rect.abs();
            let target = match plink.target {
                PdfrumLinkTarget::Uri(uri) if !uri.is_empty() => LinkTarget::External { uri },
                PdfrumLinkTarget::Page(page_idx) => {
                    let target_page = page_idx.get();
                    let y_pts = dest_y_pts(&raw, resolver);
                    let y_frac = y_pts.and_then(|y| {
                        let th = page_heights
                            .get(target_page as usize)
                            .copied()
                            .unwrap_or(0.0);
                        (th > 0.0).then(|| ((th - y) / th).clamp(0.0, 1.0))
                    });
                    LinkTarget::Internal {
                        page: target_page,
                        y_frac,
                    }
                }
                PdfrumLinkTarget::Other | PdfrumLinkTarget::Uri(_) | _ => continue,
            };

            result.push(ExtractedLink {
                page: i,
                rect_pts: [
                    rect.x0 as f32,
                    rect.y0 as f32,
                    rect.x1 as f32,
                    rect.y1 as f32,
                ],
                page_width_pts: pw,
                page_height_pts: ph,
                target,
            });
        }
    }

    Ok(result)
}

fn annot_color_hex(dict: &pdfrum::Dict) -> String {
    let Some(Object::Array(arr)) = dict.raw(&Name::from("C")) else {
        return "#ffff00".to_string();
    };
    match arr.len() {
        1 => {
            let g = (arr.number_at_or_zero(0) * 255.0).clamp(0.0, 255.0) as u8;
            format!("#{g:02x}{g:02x}{g:02x}")
        }
        3 => {
            let r = (arr.number_at_or_zero(0) * 255.0).clamp(0.0, 255.0) as u8;
            let g = (arr.number_at_or_zero(1) * 255.0).clamp(0.0, 255.0) as u8;
            let b = (arr.number_at_or_zero(2) * 255.0).clamp(0.0, 255.0) as u8;
            format!("#{r:02x}{g:02x}{b:02x}")
        }
        4 => {
            // DeviceCMYK → RGB (multiplicative, matching pdfrum annot_rgb_bytes)
            let c = arr.number_at_or_zero(0);
            let m = arr.number_at_or_zero(1);
            let y = arr.number_at_or_zero(2);
            let k = arr.number_at_or_zero(3);
            let r = ((1.0 - c) * (1.0 - k) * 255.0).clamp(0.0, 255.0) as u8;
            let g = ((1.0 - m) * (1.0 - k) * 255.0).clamp(0.0, 255.0) as u8;
            let b = ((1.0 - y) * (1.0 - k) * 255.0).clamp(0.0, 255.0) as u8;
            format!("#{r:02x}{g:02x}{b:02x}")
        }
        _ => "#ffff00".to_string(),
    }
}

/// Pulls the destination Y (PDF points, bottom-up) from a link's `/Dest` or
/// action `/D` array when present.
fn dest_y_pts(link: &pdfrum::Link, resolver: &impl pdfrum::Resolve) -> Option<f32> {
    let array = link.dict.array(&Name::from("Dest"), resolver).or_else(|| {
        let action = link.dict.dict(&Name::from("A"), resolver)?;
        action.array(&Name::from("D"), resolver)
    })?;
    let dest = Dest { array: Some(array) };
    if let Some(xyz) = dest.xyz(resolver) {
        return xyz.y;
    }
    // FitH / FitBH: a single top parameter; XYZ-like fallbacks use index 1.
    let params = dest.params_all();
    match params.len() {
        1 => Some(params[0]),
        n if n >= 2 => Some(params[1]),
        _ => None,
    }
}

/// Where a [`ExtractedLink`] points.
#[derive(Debug, Clone)]
pub enum LinkTarget {
    /// A jump to another location in the same document.
    Internal {
        /// Zero-based destination page.
        page: u32,
        /// Target position down the destination page as a top-down fraction
        /// (0..1) of its height, when the destination specifies one. `None`
        /// falls back to a page-top jump.
        y_frac: Option<f32>,
    },
    /// A link to an external resource (web URL, DOI, `mailto:`, etc.).
    External {
        /// The raw URI string from the PDF's URI action.
        uri: String,
    },
}

/// A link extracted from a PDF page, with the source rectangle in PDF point
/// coordinates and its resolved [`LinkTarget`].
#[derive(Debug, Clone)]
pub struct ExtractedLink {
    /// Zero-based source page where the clickable rectangle appears.
    pub page: u32,
    /// [x1 (left), y1 (bottom), x2 (right), y2 (top)] source rect in PDF points.
    pub rect_pts: [f32; 4],
    /// Width of the source page in PDF points.
    pub page_width_pts: f32,
    /// Height of the source page in PDF points.
    pub page_height_pts: f32,
    /// The resolved destination.
    pub target: LinkTarget,
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
    /// [x1 (left), y1 (bottom), x2 (right), y2 (top)] union bounds in PDF points.
    pub rect_pts: [f32; 4],
    /// Per-quad `[x0, y0, x1, y1]` in PDF points for multi-quad Highlight/Underline.
    /// Empty when the annot has no `/QuadPoints`, a single quad, or a non-markup type —
    /// import then uses [`Self::rect_pts`] alone.
    pub rects_pts: Vec<[f32; 4]>,
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

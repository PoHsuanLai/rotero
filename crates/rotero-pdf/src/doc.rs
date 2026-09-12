//! PDF document cache and operations built on `pdfrum::Document`.
//!
//! `pdfrum::Document` is `Send + Sync`, so callers open via [`DocCache`] (or
//! `Document::open` directly) and call free functions with `&Document`. Each
//! render creates its own [`RenderSession`] (`&mut`-only, not shared).

use std::collections::HashMap;
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use pdfrum::{
    Argb, ColorMode, ColorScheme, Dest, Document, LinkTarget as PdfrumLinkTarget, Name, Object,
    RenderOptions, RenderSession, Subtype, VelloCpuBackend,
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
    LoadError(pdfrum::Error),
    /// The PDF is encrypted and the password was missing or wrong.
    #[error("Password required")]
    WrongPassword,
    #[error("Page {0} out of range (total: {1})")]
    PageOutOfRange(u32, u32),
    #[error("Failed to render page: {0}")]
    RenderError(String),
    #[error("Failed to encode image: {0}")]
    ImageError(String),
    #[error("Failed to write annotations: {0}")]
    WriteError(String),
}

impl From<pdfrum::Error> for PdfError {
    fn from(err: pdfrum::Error) -> Self {
        match err {
            pdfrum::Error::WrongPassword => Self::WrongPassword,
            other => Self::LoadError(other),
        }
    }
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
        self.open_with(pdf_path, None)
    }

    /// Like [`Self::open`], optionally supplying a password for encrypted PDFs.
    pub fn open_with(
        &self,
        pdf_path: &str,
        password: Option<&str>,
    ) -> Result<Arc<Document>, PdfError> {
        let mtime = file_mtime(pdf_path);
        let mut guard = self.docs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = guard.get(pdf_path)
            && cached.mtime == mtime
        {
            return Ok(Arc::clone(&cached.doc));
        }
        let doc = match password {
            Some(pw) => Document::open_with_password(pdf_path, pw.as_bytes())?,
            None => Document::open(pdf_path)?,
        };
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

/// Build render options for `scale`, optionally applying a dark / night-mode
/// colour scheme (forced light ink on a dark page background).
pub fn render_options_for(scale: f32, dark: bool) -> RenderOptions {
    let mut opts = RenderOptions::builder().scale(f64::from(scale));
    if dark {
        // Light strokes/fills on a near-black page — images stay untouched.
        let ink = Argb::opaque(220, 220, 220);
        let scheme = ColorScheme::new(ink, ink, Argb::WHITE, Argb::WHITE);
        opts = opts
            .color_mode(ColorMode::Forced(scheme))
            .background(pdfrum::Color::from_rgb8(24, 24, 28));
    }
    opts.build()
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
    render_pages_with(doc, start, count, scale, false, session)
}

/// Like [`render_pages`], with optional dark / night-mode colour scheme.
///
/// A single page reuses `session`. Multiple pages fan out across threads,
/// each with its own [`RenderSession`] (sessions are `&mut`-only and must
/// not be shared). Page order in the returned `Vec` matches the input range.
/// When `count > 1`, `session` is unused for the concurrent path.
pub fn render_pages_with(
    doc: &Document,
    start: u32,
    count: u32,
    scale: f32,
    dark: bool,
    session: &mut RenderSession,
) -> Result<Vec<RenderedPage>, PdfError> {
    let page_count = doc.page_count();
    let end = (start + count).min(page_count);
    let opts = render_options_for(scale, dark);
    let n = (end - start) as usize;
    if n == 0 {
        return Ok(Vec::new());
    }
    if n == 1 {
        // Keep the caller's session warm for single-page / sequential callers.
        return Ok(vec![render_page_inner(session, doc, start, &opts)?]);
    }

    // Parallel fan-out. `Document` is `Send + Sync`; each task owns a session.
    let _ = session;
    let mut slots: Vec<Option<Result<RenderedPage, PdfError>>> = (0..n).map(|_| None).collect();
    std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(n);
        for (slot, page_index) in (start..end).enumerate() {
            let opts = opts.clone();
            handles.push(scope.spawn(move || {
                let mut local = RenderSession::new();
                (slot, render_page_inner(&mut local, doc, page_index, &opts))
            }));
        }
        for handle in handles {
            match handle.join() {
                Ok((slot, result)) => slots[slot] = Some(result),
                Err(_) => {
                    if let Some(empty) = slots.iter_mut().find(|s| s.is_none()) {
                        *empty = Some(Err(PdfError::RenderError(
                            "page render thread panicked".into(),
                        )));
                    }
                }
            }
        }
    });

    let mut pages = Vec::with_capacity(n);
    for slot in slots {
        pages.push(slot.expect("every slot filled")?);
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
    open_and_render_initial_with(cache, pdf_path, scale, batch_size, None)
}

/// Like [`open_and_render_initial`], with an optional password for encrypted PDFs.
pub fn open_and_render_initial_with(
    cache: &DocCache,
    pdf_path: &str,
    scale: f32,
    batch_size: u32,
    password: Option<&str>,
) -> Result<(u32, Vec<RenderedPage>), PdfError> {
    open_and_render_initial_with_theme(cache, pdf_path, scale, batch_size, password, false)
}

/// Like [`open_and_render_initial_with`], with dark / night-mode rendering.
pub fn open_and_render_initial_with_theme(
    cache: &DocCache,
    pdf_path: &str,
    scale: f32,
    batch_size: u32,
    password: Option<&str>,
    dark: bool,
) -> Result<(u32, Vec<RenderedPage>), PdfError> {
    let doc = cache.open_with(pdf_path, password)?;
    let page_count = doc.page_count();
    let mut session = RenderSession::new();
    let pages = render_pages_with(&doc, 0, batch_size, scale, dark, &mut session)?;
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

/// `/PageLabels` strings for every page, or `None` when the document numbers
/// pages by ordinal only (the common case).
pub fn page_labels(doc: &Document) -> Vec<Option<String>> {
    (0..doc.page_count()).map(|i| doc.page_label(i)).collect()
}

/// Display string for a zero-based page index: the PDF label when present,
/// otherwise the 1-based ordinal.
pub fn display_page_number(labels: &[Option<String>], page_index: u32) -> String {
    labels
        .get(page_index as usize)
        .and_then(|l| l.as_ref())
        .cloned()
        .unwrap_or_else(|| (page_index + 1).to_string())
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
                Subtype::StrikeOut => rotero_models::AnnotationType::StrikeOut,
                Subtype::Squiggly => rotero_models::AnnotationType::Squiggly,
                Subtype::Ink => rotero_models::AnnotationType::Ink,
                Subtype::FreeText => rotero_models::AnnotationType::Text,
                _ => continue,
            };

            // Prefer /QuadPoints for text-markup subtypes; fall back to /Rect.
            // When multiple quads exist, keep each as rects_pts so import can
            // populate geometry.rects (same shape create/write already uses).
            let (bounds, rects_pts) = match ann_type {
                rotero_models::AnnotationType::Highlight
                | rotero_models::AnnotationType::Underline
                | rotero_models::AnnotationType::StrikeOut
                | rotero_models::AnnotationType::Squiggly => {
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

/// Reads all links from the PDF — `/Link` annotations and bare URLs / DOIs
/// discovered in page text via [`TextPage::web_links`](pdfrum::TextPage::web_links).
///
/// Identical external URI + source rect pairs are not double-counted when a
/// `/Link` annot already covers the same address.
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

        // Bare http(s)/DOI/www/mailto in body text — clickable like /Link annots.
        let text = page.text();
        for web in text.web_links() {
            let uri = web.url.trim();
            if uri.is_empty() {
                continue;
            }
            let pdf_rects = text.rects(web.range.clone());
            if pdf_rects.is_empty() {
                continue;
            }
            // One hotspot per text-object run; usually a single line for a URL.
            for rect in pdf_rects {
                let rect = rect.abs();
                let rect_pts = [
                    rect.x0 as f32,
                    rect.y0 as f32,
                    rect.x1 as f32,
                    rect.y1 as f32,
                ];
                if link_already_covers(&result, i, uri, &rect_pts) {
                    continue;
                }
                result.push(ExtractedLink {
                    page: i,
                    rect_pts,
                    page_width_pts: pw,
                    page_height_pts: ph,
                    target: LinkTarget::External {
                        uri: uri.to_string(),
                    },
                });
            }
        }
    }

    Ok(result)
}

/// True when `result` already has an external link on `page` with the same
/// URI and a nearly identical source rect (avoids double-counting web_links
/// that sit under an existing `/Link` annotation).
fn link_already_covers(
    result: &[ExtractedLink],
    page: u32,
    uri: &str,
    rect_pts: &[f32; 4],
) -> bool {
    const EPS: f32 = 1.5;
    result.iter().any(|l| {
        if l.page != page {
            return false;
        }
        let LinkTarget::External { uri: existing } = &l.target else {
            return false;
        };
        if !uris_equivalent(existing, uri) {
            return false;
        }
        (0..4).all(|i| (l.rect_pts[i] - rect_pts[i]).abs() <= EPS)
            || rects_overlap_mostly(&l.rect_pts, rect_pts)
    })
}

fn uris_equivalent(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        let s = s.trim().trim_end_matches('/');
        let lower = s.to_ascii_lowercase();
        lower
            .strip_prefix("https://")
            .or_else(|| lower.strip_prefix("http://"))
            .unwrap_or(&lower)
            .to_string()
    }
    norm(a) == norm(b)
}

/// Rough IoU-ish overlap so a slightly larger `/Link` rect still suppresses
/// the text-derived hotspot for the same URI.
fn rects_overlap_mostly(a: &[f32; 4], b: &[f32; 4]) -> bool {
    let ax0 = a[0].min(a[2]);
    let ay0 = a[1].min(a[3]);
    let ax1 = a[0].max(a[2]);
    let ay1 = a[1].max(a[3]);
    let bx0 = b[0].min(b[2]);
    let by0 = b[1].min(b[3]);
    let bx1 = b[0].max(b[2]);
    let by1 = b[1].max(b[3]);
    let ix0 = ax0.max(bx0);
    let iy0 = ay0.max(by0);
    let ix1 = ax1.min(bx1);
    let iy1 = ay1.min(by1);
    let iw = (ix1 - ix0).max(0.0);
    let ih = (iy1 - iy0).max(0.0);
    let inter = iw * ih;
    if inter <= 0.0 {
        return false;
    }
    let area_b = ((bx1 - bx0) * (by1 - by0)).max(1e-3);
    inter / area_b >= 0.7
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
    /// Per-quad `[x0, y0, x1, y1]` in PDF points for multi-quad text markup.
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

#[cfg(test)]
mod page_label_tests {
    use super::{display_page_number, page_labels};
    use pdfrum::Document;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    #[test]
    fn display_page_number_falls_back_to_ordinal() {
        let labels = vec![None, Some("iv".into()), None];
        assert_eq!(display_page_number(&labels, 0), "1");
        assert_eq!(display_page_number(&labels, 1), "iv");
        assert_eq!(display_page_number(&labels, 2), "3");
        assert_eq!(display_page_number(&labels, 99), "100");
    }

    #[test]
    fn page_labels_len_matches_page_count() {
        let doc = Document::open(fixture("basicapi.pdf")).expect("open");
        let labels = page_labels(&doc);
        assert_eq!(labels.len(), doc.page_count() as usize);
    }
}


#[cfg(test)]
mod render_parallel_tests {
    use super::{render_pages, render_pages_with, DocCache};
    use pdfrum::RenderSession;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    #[test]
    fn render_pages_with_preserves_order_when_fanned_out() {
        let cache = DocCache::new();
        let path = fixture("tracemonkey.pdf");
        let doc = cache.open(path.to_str().unwrap()).expect("open");
        let count = doc.page_count().min(4).max(2);
        let mut session = RenderSession::new();
        let pages = render_pages_with(&doc, 0, count, 0.5, false, &mut session)
            .expect("parallel render");
        assert_eq!(pages.len(), count as usize);
        for (i, page) in pages.iter().enumerate() {
            assert_eq!(page.page_index, i as u32);
            assert!(!page.base64_data.is_empty());
            assert!(page.width > 0 && page.height > 0);
        }

        // Dark path still works under fan-out.
        let dark = render_pages_with(&doc, 0, count, 0.5, true, &mut session)
            .expect("dark parallel render");
        assert_eq!(dark.len(), count as usize);

        // Single-page path still accepts the shared session.
        let one = render_pages(&doc, 0, 1, 0.5, &mut session).expect("single");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].page_index, 0);
    }
}

#[cfg(test)]
mod password_tests {
    use super::{DocCache, PdfError};
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    #[test]
    fn wrong_password_maps_to_pdf_error_variant() {
        let cache = DocCache::new();
        let path = fixture("encrypted.pdf");
        let err = cache
            .open(path.to_str().unwrap())
            .expect_err("needs password");
        assert!(matches!(err, PdfError::WrongPassword), "got {err:?}");
        let err = cache
            .open_with(path.to_str().unwrap(), Some("nope"))
            .expect_err("wrong password");
        assert!(matches!(err, PdfError::WrongPassword), "got {err:?}");
        let doc = cache
            .open_with(path.to_str().unwrap(), Some("1234"))
            .expect("correct password");
        assert!(doc.page_count() >= 1);
    }
}

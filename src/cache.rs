//! Disk cache for rendered PDF pages and extracted text.
//!
//! Layout: `{data_dir}/cache/{cache_key}/`
//!   - `meta.json`        — CacheMeta (page_count, zoom, mtime)
//!   - `pages/{n}.jpg`    — rendered page JPEG (raw bytes, not base64)
//!   - `text.json`        — Vec<PageTextData> serialized

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use rotero_pdf::PageTextData;
use serde::{Deserialize, Serialize};

use crate::state::app_state::RenderedPageData;

/// Current text extraction version. Bump to invalidate cached text data.
const TEXT_VERSION: u32 = 4;

/// Metadata stored alongside cached pages.
#[derive(Debug, Serialize, Deserialize)]
pub struct CacheMeta {
    pub page_count: u32,
    pub zoom: f32,
    /// File modification time as seconds since epoch, for invalidation.
    pub pdf_mtime: u64,
    /// Dimensions (width, height) per page at the cached zoom level.
    pub page_dims: Vec<(u32, u32)>,
    /// Text extraction version for cache invalidation.
    #[serde(default)]
    pub text_version: u32,
    /// MIME type of cached images (e.g. "image/jpeg" or "image/png").
    #[serde(default = "default_mime")]
    pub mime: String,
}

fn default_mime() -> String {
    "image/png".to_string()
}

fn ext_for_mime(_mime: &str) -> &str {
    "png"
}

/// Extract (width, height) from PNG or JPEG image bytes by reading the header.
fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // PNG: bytes 16-19 = width, 20-23 = height (in IHDR chunk)
    if bytes.len() > 24 && bytes.starts_with(b"\x89PNG") {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        return Some((w, h));
    }
    // JPEG: scan for SOF0/SOF2 marker (0xFF 0xC0 or 0xFF 0xC2)
    if bytes.len() > 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = bytes[i + 1];
            if marker == 0xC0 || marker == 0xC2 {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            // Skip marker segment
            if i + 3 < bytes.len() {
                let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
                i += 2 + len;
            } else {
                break;
            }
        }
    }
    None
}

/// Get a rotero-cache:// URL for a cached page image, if it exists on disk.
/// The URL is served by a custom WebView protocol handler registered at launch.
pub fn page_file_url(data_dir: &Path, pdf_path: &str, page_index: u32, mime: &str) -> Option<String> {
    let hash = simple_hash(pdf_path);
    let ext = ext_for_mime(mime);
    let file_path = data_dir.join("cache").join(&hash).join("pages").join(format!("{page_index}.{ext}"));
    if file_path.exists() {
        Some(format!("rotero-cache://{hash}/pages/{page_index}.{ext}"))
    } else {
        None
    }
}

/// Get the cache directory for a PDF.
fn cache_dir(data_dir: &Path, pdf_path: &str) -> PathBuf {
    // Use a hash of the PDF path as the cache key
    let hash = simple_hash(pdf_path);
    data_dir.join("cache").join(hash)
}

fn simple_hash(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn pdf_mtime(pdf_path: &str) -> u64 {
    fs::metadata(pdf_path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Check if a valid cache exists for this PDF at the given zoom level.
pub fn load_cached(
    data_dir: &Path,
    pdf_path: &str,
    zoom: f32,
) -> Option<(CacheMeta, Vec<RenderedPageData>)> {
    let dir = cache_dir(data_dir, pdf_path);
    let meta_path = dir.join("meta.json");
    let meta_str = fs::read_to_string(&meta_path).ok()?;
    let meta: CacheMeta = serde_json::from_str(&meta_str).ok()?;

    // Validate: same zoom, PDF not modified since cache
    let current_mtime = pdf_mtime(pdf_path);
    if (meta.zoom - zoom).abs() > 0.01 || meta.pdf_mtime != current_mtime {
        return None;
    }

    let pages_dir = dir.join("pages");
    let ext = ext_for_mime(&meta.mime);
    let mime: &'static str = if meta.mime == "image/png" {
        "image/png"
    } else {
        "image/jpeg"
    };
    let mut pages = Vec::with_capacity(meta.page_count as usize);
    for i in 0..meta.page_count {
        let img_path = pages_dir.join(format!("{i}.{ext}"));
        let bytes = match fs::read(&img_path) {
            Ok(b) => b,
            Err(_) => continue, // Skip missing pages — they'll be lazy-loaded on scroll
        };
        let (mut w, mut h) = meta.page_dims.get(i as usize).copied().unwrap_or((0, 0));
        // Recover dimensions from image header if metadata was corrupted by a race condition
        if w == 0 || h == 0 {
            if let Some((iw, ih)) = image_dimensions(&bytes) {
                w = iw;
                h = ih;
            }
        }
        if w == 0 || h == 0 {
            continue; // Can't display without dimensions
        }
        let base64_data =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        pages.push(RenderedPageData {
            page_index: i,
            base64_data: std::sync::Arc::new(base64_data),
            mime,
            width: w,
            height: h,
        });
    }

    // Only return cache hit if we loaded at least some pages
    if pages.is_empty() {
        return None;
    }

    Some((meta, pages))
}

/// Load cached text data if available.
pub fn load_cached_text(data_dir: &Path, pdf_path: &str) -> Option<HashMap<u32, PageTextData>> {
    let dir = cache_dir(data_dir, pdf_path);
    // Verify meta still valid
    let meta_str = fs::read_to_string(dir.join("meta.json")).ok()?;
    let meta: CacheMeta = serde_json::from_str(&meta_str).ok()?;
    if meta.pdf_mtime != pdf_mtime(pdf_path) || meta.text_version != TEXT_VERSION {
        return None;
    }

    let text_str = fs::read_to_string(dir.join("text.json")).ok()?;
    let text_vec: Vec<PageTextData> = serde_json::from_str(&text_str).ok()?;
    Some(text_vec.into_iter().map(|t| (t.page_index, t)).collect())
}

/// Save rendered pages to cache.
///
/// This merges incrementally: new page images are written to disk and
/// `page_dims` in the metadata is extended (not replaced) so that
/// subsequent batches from scroll-loading accumulate correctly.
pub fn save_pages(
    data_dir: &Path,
    pdf_path: &str,
    zoom: f32,
    page_count: u32,
    pages: &[RenderedPageData],
) {
    let dir = cache_dir(data_dir, pdf_path);
    let pages_dir = dir.join("pages");
    let _ = fs::create_dir_all(&pages_dir);

    // Determine format from first page's mime
    let mime = pages.first().map(|p| p.mime).unwrap_or("image/jpeg");
    let ext = ext_for_mime(mime);

    // Write page images
    for page in pages {
        if let Ok(bytes) = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            page.base64_data.as_str(),
        ) {
            let _ = fs::write(pages_dir.join(format!("{}.{ext}", page.page_index)), &bytes);
        }
    }

    // Re-read metadata right before writing to minimise the race window with
    // concurrent save threads. Any page_dims entry that is already non-zero is
    // preserved (not overwritten with (0,0) from resize).
    let mtime = pdf_mtime(pdf_path);
    let mut page_dims: Vec<(u32, u32)> = fs::read_to_string(dir.join("meta.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<CacheMeta>(&s).ok())
        .filter(|m| (m.zoom - zoom).abs() < 0.01 && m.pdf_mtime == mtime)
        .map(|m| m.page_dims)
        .unwrap_or_default();

    // Extend page_dims to fit the highest page index in this batch
    for page in pages {
        let idx = page.page_index as usize;
        if idx >= page_dims.len() {
            page_dims.resize(idx + 1, (0, 0));
        }
        page_dims[idx] = (page.width, page.height);
    }

    // Fill any remaining (0,0) gaps by probing page files on disk.
    // This recovers dimensions lost due to race conditions between
    // concurrent save_pages threads.
    for (i, dims) in page_dims.iter_mut().enumerate() {
        if *dims == (0, 0) {
            let img_path = pages_dir.join(format!("{i}.{ext}"));
            if let Ok(bytes) = fs::read(&img_path) {
                if let Some(sz) = image_dimensions(&bytes) {
                    *dims = sz;
                }
            }
        }
    }

    let meta = CacheMeta {
        page_count,
        zoom,
        pdf_mtime: mtime,
        page_dims,
        text_version: TEXT_VERSION,
        mime: mime.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&meta) {
        let _ = fs::write(dir.join("meta.json"), json);
    }
}


/// Save extracted text to cache.
pub fn save_text(data_dir: &Path, pdf_path: &str, text_data: &HashMap<u32, PageTextData>) {
    let dir = cache_dir(data_dir, pdf_path);
    let _ = fs::create_dir_all(&dir);

    let text_vec: Vec<&PageTextData> = text_data.values().collect();
    if let Ok(json) = serde_json::to_string(&text_vec) {
        let _ = fs::write(dir.join("text.json"), json);
    }
}

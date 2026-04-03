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
const TEXT_VERSION: u32 = 3;

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
    let mut pages = Vec::with_capacity(meta.page_count as usize);
    for i in 0..meta.page_count {
        let jpg_path = pages_dir.join(format!("{i}.jpg"));
        let bytes = match fs::read(&jpg_path) {
            Ok(b) => b,
            Err(_) => break, // Stop at first missing page — remaining pages will be lazy-loaded
        };
        let base64_data =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        let (w, h) = meta.page_dims.get(i as usize).copied().unwrap_or((0, 0));
        pages.push(RenderedPageData {
            page_index: i,
            base64_data: std::sync::Arc::new(base64_data),
            mime: "image/jpeg",
            width: w,
            height: h,
            quality: 85,
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

    // Write page images
    for page in pages {
        if let Ok(bytes) = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            page.base64_data.as_str(),
        ) {
            let _ = fs::write(pages_dir.join(format!("{}.jpg", page.page_index)), &bytes);
        }
    }

    // Load existing metadata to merge page_dims (don't clobber previously cached pages)
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

    let meta = CacheMeta {
        page_count,
        zoom,
        pdf_mtime: mtime,
        page_dims,
        text_version: TEXT_VERSION,
    };
    if let Ok(json) = serde_json::to_string(&meta) {
        let _ = fs::write(dir.join("meta.json"), json);
    }
}

/// Load a single page from the disk cache (for re-loading evicted pages).
#[allow(dead_code)]
pub fn load_single_page(
    data_dir: &Path,
    pdf_path: &str,
    page_index: u32,
    zoom: f32,
) -> Option<RenderedPageData> {
    let dir = cache_dir(data_dir, pdf_path);
    let meta_str = fs::read_to_string(dir.join("meta.json")).ok()?;
    let meta: CacheMeta = serde_json::from_str(&meta_str).ok()?;

    // Validate zoom + mtime
    if (meta.zoom - zoom).abs() > 0.01 || meta.pdf_mtime != pdf_mtime(pdf_path) {
        return None;
    }

    let jpg_path = dir.join("pages").join(format!("{page_index}.jpg"));
    let bytes = fs::read(&jpg_path).ok()?;
    let base64_data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    let (w, h) = meta
        .page_dims
        .get(page_index as usize)
        .copied()
        .unwrap_or((0, 0));
    Some(RenderedPageData {
        page_index,
        base64_data: std::sync::Arc::new(base64_data),
        mime: "image/jpeg",
        width: w,
        height: h,
        quality: 85,
    })
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

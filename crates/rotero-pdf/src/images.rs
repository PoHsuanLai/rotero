//! Page figure / embedded image listing and PNG extraction.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use pdfrum::Document;
use serde::{Deserialize, Serialize};

use crate::doc::PdfError;

/// One figure extracted from a page's image XObjects / inline images.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageFigure {
    /// Zero-based page index.
    pub page_index: u32,
    /// Index into [`Page::images`](pdfrum::Page::images) for this page.
    pub image_index: u32,
    pub width: u32,
    pub height: u32,
    /// True for stencil masks (usually skip for "save figure").
    pub is_mask: bool,
    /// PNG bytes as base64 when extracted; `None` for list-only results.
    pub png_base64: Option<String>,
}

/// List figures on a page (no pixel payload).
pub fn list_page_images(doc: &Document, page_index: u32) -> Result<Vec<PageFigure>, PdfError> {
    let page_count = doc.page_count();
    if page_index >= page_count {
        return Err(PdfError::PageOutOfRange(page_index, page_count));
    }
    let page = doc.page(page_index)?;
    let images = page.images();
    Ok(images
        .into_iter()
        .enumerate()
        .map(|(i, img)| PageFigure {
            page_index,
            image_index: i as u32,
            width: img.width,
            height: img.height,
            is_mask: img.is_mask,
            png_base64: None,
        })
        .collect())
}

/// Extract one figure as PNG (base64).
pub fn extract_page_image_png(
    doc: &Document,
    page_index: u32,
    image_index: u32,
) -> Result<PageFigure, PdfError> {
    let page_count = doc.page_count();
    if page_index >= page_count {
        return Err(PdfError::PageOutOfRange(page_index, page_count));
    }
    let page = doc.page(page_index)?;
    let images = page.images();
    let img = images.get(image_index as usize).ok_or_else(|| {
        PdfError::RenderError(format!(
            "image {image_index} out of range ({} on page {page_index})",
            images.len()
        ))
    })?;
    let pixmap = img.pixmap();
    let png = pixmap
        .encode_png()
        .map_err(|e| PdfError::ImageError(e.to_string()))?;
    Ok(PageFigure {
        page_index,
        image_index,
        width: img.width,
        height: img.height,
        is_mask: img.is_mask,
        png_base64: Some(BASE64.encode(&png)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfs")
            .join(name)
    }

    #[test]
    fn list_page_images_runs_on_fixture() {
        let doc = Document::open(fixture("basicapi.pdf")).expect("open");
        let figs = list_page_images(&doc, 0).expect("list");
        // May be empty; must not error.
        for f in &figs {
            assert_eq!(f.page_index, 0);
            assert!(f.png_base64.is_none());
        }
    }
}

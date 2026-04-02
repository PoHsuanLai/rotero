use dioxus::prelude::*;
use rotero_pdf::PdfEngine;

use super::app_state::{PdfViewState, RenderedPageData};

/// Open a PDF file and render its pages.
pub fn open_pdf(
    engine: &PdfEngine,
    state: &mut Signal<PdfViewState>,
    pdf_path: &str,
) -> Result<(), String> {
    let info = engine
        .load_document(pdf_path)
        .map_err(|e| e.to_string())?;

    let zoom = state.read().zoom;

    // Render first batch of pages (up to 5 initially)
    let render_count = info.page_count.min(5);
    let rendered = engine
        .render_pages(pdf_path, 0, render_count, zoom)
        .map_err(|e| e.to_string())?;

    let pages: Vec<RenderedPageData> = rendered.into_iter().map(|r| r.into()).collect();

    state.set(PdfViewState {
        pdf_path: Some(pdf_path.to_string()),
        page_count: info.page_count,
        current_page: 0,
        zoom,
        rendered_pages: pages,
        ..PdfViewState::new()
    });

    Ok(())
}

/// Render additional pages (for lazy loading on scroll).
pub fn render_more_pages(
    engine: &PdfEngine,
    state: &mut Signal<PdfViewState>,
    start: u32,
    count: u32,
) -> Result<(), String> {
    let s = state.read();
    let pdf_path = s.pdf_path.clone();
    let zoom = s.zoom;
    drop(s);

    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    let rendered = engine
        .render_pages(&pdf_path, start, count, zoom)
        .map_err(|e| e.to_string())?;

    let new_pages: Vec<RenderedPageData> = rendered.into_iter().map(|r| r.into()).collect();

    state.with_mut(|s| {
        s.rendered_pages.extend(new_pages);
    });

    Ok(())
}

/// Change zoom level and re-render all loaded pages.
pub fn set_zoom(
    engine: &PdfEngine,
    state: &mut Signal<PdfViewState>,
    new_zoom: f32,
) -> Result<(), String> {
    let s = state.read();
    let pdf_path = s.pdf_path.clone();
    let page_count = s.rendered_pages.len() as u32;
    drop(s);

    let Some(pdf_path) = pdf_path else {
        return Ok(());
    };

    let rendered = engine
        .render_pages(&pdf_path, 0, page_count, new_zoom)
        .map_err(|e| e.to_string())?;

    let pages: Vec<RenderedPageData> = rendered.into_iter().map(|r| r.into()).collect();

    state.with_mut(|s| {
        s.zoom = new_zoom;
        s.rendered_pages = pages;
    });

    Ok(())
}

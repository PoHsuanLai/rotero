use dioxus::prelude::*;

use crate::state::app_state::PdfViewState;

#[component]
pub fn PdfViewer() -> Element {
    let pdf_state = use_context::<Signal<PdfViewState>>();

    let state = pdf_state.read();

    if state.pdf_path.is_none() {
        return rsx! {
            div {
                style: "flex: 1; display: flex; align-items: center; justify-content: center; color: #999; font-size: 16px;",
                "Open a PDF to get started"
            }
        };
    }

    let page_count = state.page_count;
    let zoom = state.zoom;

    rsx! {
        div { class: "pdf-viewer-container",
            style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",

            // Toolbar
            PdfToolbar { page_count, zoom }

            // Scrollable page area
            div { class: "pdf-pages",
                style: "flex: 1; overflow-y: auto; background: #e8e8e8; padding: 16px; display: flex; flex-direction: column; align-items: center; gap: 12px;",
                for page in state.rendered_pages.iter() {
                    div { class: "pdf-page",
                        style: "background: white; box-shadow: 0 2px 8px rgba(0,0,0,0.15);",
                        img {
                            src: "data:image/png;base64,{page.base64_png}",
                            width: "{page.width}",
                            height: "{page.height}",
                            style: "display: block;",
                        }
                    }
                }

                // Load more indicator
                if (state.rendered_pages.len() as u32) < page_count {
                    LoadMorePages {}
                }
            }
        }
    }
}

#[component]
fn PdfToolbar(page_count: u32, zoom: f32) -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let zoom_percent = (zoom * 100.0 / 1.5) as u32; // 1.5 = 100%

    rsx! {
        div { class: "pdf-toolbar",
            style: "display: flex; align-items: center; gap: 12px; padding: 8px 16px; background: #fff; border-bottom: 1px solid #ddd; font-size: 13px;",

            span { style: "color: #666;", "{page_count} pages" }

            div { style: "flex: 1;" }

            // Zoom controls
            button {
                style: "padding: 4px 8px; border: 1px solid #ddd; background: #fff; cursor: pointer; border-radius: 4px;",
                onclick: move |_| {
                    let current_zoom = pdf_state.read().zoom;
                    let new_zoom = (current_zoom - 0.3).max(0.5);
                    pdf_state.with_mut(|s| s.zoom = new_zoom);
                },
                "-"
            }
            span { style: "min-width: 50px; text-align: center;", "{zoom_percent}%" }
            button {
                style: "padding: 4px 8px; border: 1px solid #ddd; background: #fff; cursor: pointer; border-radius: 4px;",
                onclick: move |_| {
                    let current_zoom = pdf_state.read().zoom;
                    let new_zoom = (current_zoom + 0.3).min(5.0);
                    pdf_state.with_mut(|s| s.zoom = new_zoom);
                },
                "+"
            }
        }
    }
}

#[component]
fn LoadMorePages() -> Element {
    rsx! {
        div {
            style: "padding: 16px; text-align: center; color: #999;",
            "Scroll to load more pages..."
        }
    }
}

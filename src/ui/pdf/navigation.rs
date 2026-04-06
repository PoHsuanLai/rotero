use dioxus::prelude::*;

use crate::app::RenderChannel;
use crate::state::app_state::PdfTabManager;

#[component]
pub(crate) fn ThumbnailSidebar() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let render_ch = use_context::<RenderChannel>();
    let mut is_loading_thumbs = use_signal(|| false);

    let mgr = tabs.read();
    let tab = mgr.tab();
    let page_count = tab.page_count;
    let tab_id = tab.id;
    let thumbnails = tab.render.thumbnails.clone();
    drop(mgr);

    rsx! {
        div {
            class: "thumbnail-sidebar",
            onscroll: move |_| {
                if is_loading_thumbs() { return; }
                is_loading_thumbs.set(true);
                spawn(async move {
                    // Estimate which thumbnail is visible from scroll position
                    let mut eval = document::eval(
                        "(function() { let el = document.querySelector('.thumbnail-sidebar'); \
                         if (!el) return 0.0; \
                         return el.scrollTop / Math.max(el.scrollHeight, 1); })()"
                    );
                    let ratio = eval.recv::<f64>().await.unwrap_or(0.0);
                    let center = (ratio * page_count as f64) as u32;
                    let start = center.saturating_sub(25);
                    let render_tx = render_ch.sender();
                    let quality = config.read().thumbnail_quality;
                    let _ = crate::state::commands::load_thumbnails(
                        &render_tx, &mut tabs, tab_id, start, 50, quality,
                    ).await;
                    is_loading_thumbs.set(false);
                });
            },

            for page_idx in 0..page_count {
                if let Some(thumb) = thumbnails.get(&page_idx) {
                    {
                        let base64 = thumb.base64_data.clone();
                        let mime = thumb.mime;
                        let w = thumb.width;
                        let h = thumb.height;
                        let page_num = page_idx + 1;
                        rsx! {
                            div {
                                key: "thumb-{page_idx}", class: "thumbnail-item",
                                onclick: move |_| {
                                    spawn(async move {
                                        let js = format!("let pages = document.querySelectorAll('.pdf-page-wrapper'); if (pages[{page_idx}]) {{ pages[{page_idx}].scrollIntoView({{ behavior: 'smooth', block: 'start' }}); }}");
                                        let _ = document::eval(&js);
                                    });
                                },
                                img { class: "thumbnail-img", src: "data:{mime};base64,{base64}", width: "{w}", height: "{h}" }
                                span { class: "thumbnail-page-num", "{page_num}" }
                            }
                        }
                    }
                } else {
                    // Placeholder for unloaded thumbnail
                    div {
                        key: "thumb-{page_idx}", class: "thumbnail-item thumbnail-placeholder",
                        style: "width: 120px; height: 160px; background: var(--bg-secondary, #e0e0e0);",
                        {
                            let num = page_idx + 1;
                            rsx! { span { class: "thumbnail-page-num", "{num}" } }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn OutlinePanel() -> Element {
    let tabs = use_context::<Signal<PdfTabManager>>();
    let outline = tabs.read().tab().nav.outline.clone();

    rsx! {
        div { class: "outline-panel",
            div { class: "outline-panel-header", "Table of Contents" }
            div { class: "outline-panel-list",
                for (idx, entry) in outline.iter().enumerate() {
                    {
                        let indent = entry.level as f64 * 16.0;
                        let page_idx = entry.page_index;
                        let title = entry.title.clone();
                        rsx! {
                            div {
                                key: "outline-{idx}", class: "outline-entry", style: "padding-left: {indent}px;",
                                onclick: move |_| {
                                    if let Some(pi) = page_idx {
                                        spawn(async move {
                                            let js = format!("let pages = document.querySelectorAll('.pdf-page-wrapper'); if (pages[{pi}]) {{ pages[{pi}].scrollIntoView({{ behavior: 'smooth', block: 'start' }}); }}");
                                            let _ = document::eval(&js);
                                        });
                                    }
                                },
                                "{title}"
                                if let Some(pi) = page_idx {
                                    span { class: "outline-page-num", " p.{pi + 1}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

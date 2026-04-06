use dioxus::prelude::*;

use crate::state::app_state::{LibraryState, LibraryView, PdfTab, PdfTabManager};

#[component]
pub(crate) fn OpenPdfButton() -> Element {
    let mut tabs = use_context::<Signal<PdfTabManager>>();
    let mut lib_state = use_context::<Signal<LibraryState>>();
    let config = use_context::<Signal<crate::sync::engine::SyncConfig>>();
    let dpr_sig = use_context::<Signal<crate::app::DevicePixelRatio>>();
    let error_msg = use_signal(|| None::<String>);

    rsx! {
        button {
            class: "sidebar-open-btn",
            onclick: move |_| {
                spawn(async move {
                let file = crate::ui::pick_file_async(&["pdf"], "Open PDF").await;

                if let Some(path) = file {
                    let path_str = path.to_string_lossy().to_string();
                    tabs.with_mut(|m| {
                        if let Some(idx) = m.find_by_path(&path_str) {
                            let tid = m.tabs[idx].id;
                            m.switch_to(tid);
                        } else {
                            let cfg = config.read();
                            let id = m.next_id();
                            let title = std::path::Path::new(&path_str)
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "Untitled".to_string());
                            let tab = PdfTab::new(id, path_str.clone(), title, cfg.default_zoom, cfg.page_batch_size, dpr_sig.read().0);
                            m.open_tab(tab);
                        }
                    });
                    lib_state.with_mut(|s| s.view = LibraryView::PdfViewer);
                }
                });
            },
            "Open PDF"
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { class: "sidebar-error", "{err}" }
        }
    }
}

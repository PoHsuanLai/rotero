use dioxus::prelude::*;

use crate::sync::engine::SyncConfig;
use crate::ui::chat_panel::ResizeHandle;

/// The right-hand detail panel shell shared by the library detail, web-result
/// preview, and multi-select summary.
///
/// Owns the left-edge resize handle and a scrolling body. The handle is a
/// sibling of the scroll area (not inside it), so it stays pinned over the full
/// panel height even when the content scrolls — otherwise a tall panel leaves
/// its lower edge un-draggable.
///
/// The panel width is persisted to [`SyncConfig`]: it opens at the saved width
/// and writes the new width back when a drag ends, so the size survives panel
/// switches and restarts.
#[component]
pub fn DetailShell(children: Element) -> Element {
    let mut config = use_context::<Signal<SyncConfig>>();
    let width = config.read().ui.detail_width;

    rsx! {
        div {
            class: "paper-detail",
            style: "width: {width}px; min-width: {width}px;",
            ResizeHandle {
                target: "detail",
                on_resize: move |w: f64| {
                    config.with_mut(|cfg| {
                        cfg.ui.detail_width = w as f32;
                        let _ = cfg.save();
                    });
                },
            }
            div { class: "paper-detail-body",
                {children}
            }
        }
    }
}

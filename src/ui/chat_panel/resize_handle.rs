use dioxus::prelude::*;

/// Which way dragging the handle grows the target element.
///
/// - `GrowLeft`: the handle is on the target's *left* edge and lives to the right
///   of it (right-side panels like chat/detail). Dragging left widens it.
/// - `GrowRight`: the handle is on the target's *right* edge (left-side panels
///   like the sidebar, or a middle divider resizing the element to its left).
///   Dragging right widens it.
#[derive(Clone, Copy, PartialEq)]
pub enum ResizeDir {
    GrowLeft,
    GrowRight,
}

/// A thin draggable strip that resizes a target element's width via a
/// document-level mousemove listener (injected JS), mutating the DOM directly.
///
/// `target` is used to build the handle's CSS class (`{target}-resize-handle`).
/// `selector` is the element to resize (defaults derived from `target` for the
/// legacy chat/detail panels). `dir`, `min`, and `max` control the drag.
#[component]
pub fn ResizeHandle(
    target: String,
    #[props(default = ResizeDir::GrowLeft)] dir: ResizeDir,
    #[props(default = 280.0)] min: f64,
    #[props(default = 600.0)] max: f64,
    /// Explicit CSS selector for the element to resize. When omitted, falls back
    /// to the legacy mapping (`.paper-detail` for "detail", else `.{target}-panel`).
    #[props(default)]
    selector: Option<String>,
) -> Element {
    let handle_class = format!("{target}-resize-handle");

    rsx! {
        div {
            class: "{handle_class}",
            onmousedown: move |e| {
                e.prevent_default();
                let target = target.clone();
                let start_x = e.client_coordinates().x;
                let selector = selector.clone().unwrap_or_else(|| {
                    if target == "detail" {
                        ".paper-detail".to_string()
                    } else {
                        format!(".{target}-panel")
                    }
                });
                // `diff` is added to the starting width. GrowLeft grows as the
                // pointer moves left (startX - clientX); GrowRight the opposite.
                let diff_expr = match dir {
                    ResizeDir::GrowLeft => "startX - e.clientX",
                    ResizeDir::GrowRight => "e.clientX - startX",
                };
                spawn(async move {
                    let js = format!(
                        r#"
                        (function() {{
                            var panel = document.querySelector('{selector}');
                            if (!panel) return;
                            var startX = {start_x};
                            var startW = panel.offsetWidth;
                            function onMove(e) {{
                                var diff = {diff_expr};
                                var newW = Math.max({min}, Math.min({max}, startW + diff));
                                panel.style.width = newW + 'px';
                                panel.style.minWidth = newW + 'px';
                                panel.style.maxWidth = newW + 'px';
                                panel.style.flex = '0 0 auto';
                            }}
                            function onUp() {{
                                document.removeEventListener('mousemove', onMove);
                                document.removeEventListener('mouseup', onUp);
                                document.body.style.cursor = '';
                                document.body.style.userSelect = '';
                            }}
                            document.body.style.cursor = 'col-resize';
                            document.body.style.userSelect = 'none';
                            document.addEventListener('mousemove', onMove);
                            document.addEventListener('mouseup', onUp);
                        }})()
                        "#
                    );
                    let _ = dioxus::document::eval(&js);
                });
            },
        }
    }
}

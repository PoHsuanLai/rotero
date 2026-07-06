use dioxus::prelude::*;

/// Draggable left-edge handle that resizes a panel by directly mutating its
/// width in the DOM during the drag (cheap, no per-frame Rust round-trip).
///
/// When `on_resize` is provided, the final width is reported back once on mouse
/// up so the caller can persist it. Panels without a callback (e.g. chat) resize
/// for the session only.
#[component]
pub fn ResizeHandle(target: String, on_resize: Option<EventHandler<f64>>) -> Element {
    let handle_class = format!("{target}-resize-handle");

    rsx! {
        div {
            class: "{handle_class}",
            onmousedown: move |e| {
                e.prevent_default();
                let target = target.clone();
                let start_x = e.client_coordinates().x;
                let selector = if target == "detail" {
                    ".paper-detail".to_string()
                } else {
                    format!(".{target}-panel")
                };
                spawn(async move {
                    // Register the drag listeners and keep the eval alive by
                    // awaiting a Promise that resolves (via `dioxus.send`) only on
                    // mouse up. A synchronous IIFE would close the eval channel
                    // before `onUp` fires, dropping the final width.
                    let js = format!(
                        r#"
                        new Promise(function(resolve) {{
                            var panel = document.querySelector('{selector}');
                            if (!panel) {{ dioxus.send(0); return; }}
                            var startX = {start_x};
                            var startW = panel.offsetWidth;
                            function onMove(e) {{
                                var diff = startX - e.clientX;
                                var newW = Math.max(280, Math.min(600, startW + diff));
                                panel.style.width = newW + 'px';
                                panel.style.minWidth = newW + 'px';
                            }}
                            function onUp() {{
                                document.removeEventListener('mousemove', onMove);
                                document.removeEventListener('mouseup', onUp);
                                document.body.style.cursor = '';
                                document.body.style.userSelect = '';
                                dioxus.send(panel.offsetWidth);
                            }}
                            document.body.style.cursor = 'col-resize';
                            document.body.style.userSelect = 'none';
                            document.addEventListener('mousemove', onMove);
                            document.addEventListener('mouseup', onUp);
                        }})
                        "#
                    );
                    let mut eval = dioxus::document::eval(&js);
                    // The handle sends the final width on mouse up; forward it to
                    // the caller so it can persist the new size.
                    if let Ok(final_width) = eval.recv::<f64>().await
                        && let Some(cb) = on_resize
                    {
                        cb.call(final_width);
                    }
                });
            },
        }
    }
}

use dioxus::prelude::*;

#[component]
pub fn ResizeHandle(target: String) -> Element {
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
                    let js = format!(
                        r#"
                        (function() {{
                            var panel = document.querySelector('{selector}');
                            if (!panel) return;
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

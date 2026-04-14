use dioxus::prelude::*;

#[component]
pub fn Modal(
    title: String,
    on_close: EventHandler<()>,
    width: Option<&'static str>,
    children: Element,
) -> Element {
    let w = width.unwrap_or("480px");

    rsx! {
        div { class: "modal-overlay",
            onclick: move |_| on_close.call(()),

            div {
                class: "modal-dialog",
                style: "max-width: {w};",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "modal-header",
                    h3 { "{title}" }
                    button {
                        class: "detail-close",
                        onclick: move |_| on_close.call(()),
                        "\u{00d7}"
                    }
                }

                div { class: "modal-body",
                    {children}
                }
            }
        }
    }
}

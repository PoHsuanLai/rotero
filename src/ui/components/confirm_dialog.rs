use dioxus::prelude::*;

#[component]
pub fn ConfirmDialog(
    title: String,
    message: String,
    confirm_label: String,
    danger: Option<bool>,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let confirm_class = if danger.unwrap_or(false) {
        "btn btn--danger"
    } else {
        "btn btn--primary"
    };

    rsx! {
        div { class: "citation-overlay",
            onclick: move |_| on_cancel.call(()),

            div { class: "citation-dialog confirm-dialog",
                onclick: move |evt| evt.stop_propagation(),

                div { class: "citation-header",
                    h3 { "{title}" }
                    button {
                        class: "detail-close",
                        onclick: move |_| on_cancel.call(()),
                        "\u{00d7}"
                    }
                }

                div { class: "confirm-dialog-body",
                    p { "{message}" }
                }

                div { class: "citation-actions",
                    button {
                        class: "btn",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "{confirm_class}",
                        onclick: move |_| on_confirm.call(()),
                        "{confirm_label}"
                    }
                }
            }
        }
    }
}

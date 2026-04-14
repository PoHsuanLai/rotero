use dioxus::prelude::*;

use super::modal::Modal;

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
        Modal {
            title,
            on_close: move |_| on_cancel.call(()),
            width: "400px",

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

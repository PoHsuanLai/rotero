use dioxus::prelude::*;

#[component]
pub fn MetadataField(label: String, value: String) -> Element {
    rsx! {
        div { class: "detail-field",
            label { class: "detail-label", "{label}" }
            div { class: "detail-value", "{value}" }
        }
    }
}

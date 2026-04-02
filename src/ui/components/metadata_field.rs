use dioxus::prelude::*;

#[component]
pub fn MetadataField(label: String, value: String) -> Element {
    rsx! {
        div { class: "metadata-field", style: "margin-bottom: 8px;",
            label { style: "font-weight: bold; font-size: 13px; color: #666;", "{label}" }
            div { style: "font-size: 14px;", "{value}" }
        }
    }
}

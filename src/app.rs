use dioxus::prelude::*;

use crate::ui::layout::Layout;

#[component]
pub fn App() -> Element {
    rsx! {
        Layout {}
    }
}

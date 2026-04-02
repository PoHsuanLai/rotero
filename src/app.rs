use dioxus::prelude::*;

use crate::state::app_state::PdfViewState;
use crate::ui::layout::Layout;

#[component]
pub fn App() -> Element {
    // Provide global PDF view state to all components
    use_context_provider(|| Signal::new(PdfViewState::new()));

    rsx! {
        Layout {}
    }
}

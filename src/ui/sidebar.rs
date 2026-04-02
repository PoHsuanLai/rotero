use dioxus::prelude::*;

use crate::state::app_state::PdfViewState;

#[component]
pub fn Sidebar() -> Element {
    let pdf_state = use_context::<Signal<PdfViewState>>();
    let state = pdf_state.read();
    let has_pdf = state.pdf_path.is_some();
    let pdf_name = state
        .pdf_path
        .as_ref()
        .and_then(|p| p.rsplit('/').next())
        .unwrap_or("")
        .to_string();

    rsx! {
        div { class: "sidebar",
            style: "width: 250px; background: #f5f5f5; border-right: 1px solid #ddd; padding: 16px; overflow-y: auto; display: flex; flex-direction: column;",
            h2 { style: "margin: 0 0 16px 0; font-size: 18px;", "Rotero" }

            // Open PDF button
            OpenPdfButton {}

            if has_pdf {
                div { style: "margin-top: 12px; padding: 8px; background: #e8f4fd; border-radius: 6px; font-size: 13px;",
                    div { style: "font-weight: bold; margin-bottom: 4px;", "Current PDF" }
                    div { style: "color: #555; word-break: break-all;", "{pdf_name}" }
                }
            }

            div { style: "margin-top: 24px;",
                h3 { style: "font-size: 14px; color: #666; margin: 8px 0;", "Collections" }
                p { style: "color: #999; font-size: 13px;", "Coming in Phase 2" }
            }

            div { style: "margin-top: 16px;",
                h3 { style: "font-size: 14px; color: #666; margin: 8px 0;", "Tags" }
                p { style: "color: #999; font-size: 13px;", "Coming in Phase 2" }
            }
        }
    }
}

#[component]
fn OpenPdfButton() -> Element {
    let mut pdf_state = use_context::<Signal<PdfViewState>>();
    let mut error_msg = use_signal(|| None::<String>);

    rsx! {
        button {
            style: "width: 100%; padding: 10px; background: #2563eb; color: white; border: none; border-radius: 6px; cursor: pointer; font-size: 14px; font-weight: 500;",
            onclick: move |_| {
                use rfd::FileDialog;

                let file = FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_title("Open PDF")
                    .pick_file();

                if let Some(path) = file {
                    let path_str = path.to_string_lossy().to_string();

                    // Try to initialize PdfEngine and open the file
                    match rotero_pdf::PdfEngine::new(None) {
                        Ok(engine) => {
                            match crate::state::commands::open_pdf(&engine, &mut pdf_state, &path_str) {
                                Ok(()) => {
                                    error_msg.set(None);
                                }
                                Err(e) => {
                                    error_msg.set(Some(format!("Failed to open PDF: {e}")));
                                }
                            }
                        }
                        Err(e) => {
                            error_msg.set(Some(format!("PDFium not found: {e}")));
                        }
                    }
                }
            },
            "Open PDF"
        }

        if let Some(err) = error_msg.read().as_ref() {
            div { style: "margin-top: 8px; padding: 8px; background: #fee; border-radius: 4px; color: #c00; font-size: 12px;",
                "{err}"
            }
        }
    }
}

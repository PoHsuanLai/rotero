pub mod chat_panel;
pub mod citation_dialog;
pub mod components;
pub mod graph_view;
pub mod import_export;
#[cfg(feature = "desktop")]
pub mod keybindings;
pub mod layout;
pub mod library;
pub mod paper_detail;
pub mod pdf;
pub mod search_bar;
pub mod settings;
pub mod sidebar;

/// Async file picker. Uses rfd on desktop, apple-utils on iOS.
pub async fn pick_file_async(extensions: &[&str], _title: &str) -> Option<std::path::PathBuf> {
    #[cfg(feature = "desktop")]
    {
        rfd::FileDialog::new()
            .add_filter("File", extensions)
            .set_title(_title)
            .pick_file()
    }

    #[cfg(target_os = "ios")]
    {
        use apple_utils::file_type::FileType;
        use apple_utils::ios::FilePicker;

        let filters = extensions
            .iter()
            .map(|ext| FileType::Extension((*ext).to_string()))
            .collect();
        let picker = FilePicker {
            filters,
            ..Default::default()
        };
        let paths: Vec<std::path::PathBuf> = picker.open().await;
        paths.into_iter().next()
    }

    #[cfg(not(any(feature = "desktop", target_os = "ios")))]
    {
        None
    }
}

#[cfg(feature = "desktop")]
pub fn pick_file(extensions: &[&str], title: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("File", extensions)
        .set_title(title)
        .pick_file()
}

#[cfg(not(feature = "desktop"))]
pub fn pick_file(_extensions: &[&str], _title: &str) -> Option<std::path::PathBuf> {
    None
}

#[cfg(feature = "desktop")]
pub fn pick_folder(title: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new().set_title(title).pick_folder()
}

#[cfg(not(feature = "desktop"))]
pub fn pick_folder(_title: &str) -> Option<std::path::PathBuf> {
    None
}

#[cfg(feature = "desktop")]
pub fn save_file(
    extensions: &[&str],
    title: &str,
    default_name: &str,
) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("File", extensions)
        .set_title(title)
        .set_file_name(default_name)
        .save_file()
}

#[cfg(not(feature = "desktop"))]
pub fn save_file(
    _extensions: &[&str],
    _title: &str,
    _default_name: &str,
) -> Option<std::path::PathBuf> {
    None
}

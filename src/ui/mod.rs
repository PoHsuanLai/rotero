pub mod components;
#[cfg(feature = "desktop")]
pub mod keybindings;
pub mod layout;
pub mod sidebar;
pub mod library_view;
pub mod paper_detail;
pub mod pdf_viewer;
pub mod search_bar;
pub mod import_export;
pub mod citation_dialog;
pub mod settings;

/// Platform file dialog helpers. No-ops on mobile.
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
    rfd::FileDialog::new()
        .set_title(title)
        .pick_folder()
}

#[cfg(not(feature = "desktop"))]
pub fn pick_folder(_title: &str) -> Option<std::path::PathBuf> {
    None
}

#[cfg(feature = "desktop")]
pub fn save_file(extensions: &[&str], title: &str, default_name: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter("File", extensions)
        .set_title(title)
        .set_file_name(default_name)
        .save_file()
}

#[cfg(not(feature = "desktop"))]
pub fn save_file(_extensions: &[&str], _title: &str, _default_name: &str) -> Option<std::path::PathBuf> {
    None
}

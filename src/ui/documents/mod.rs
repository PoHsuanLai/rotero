//! The Documents feature: a list of authored documents and a split-pane editor
//! (Markdown left, compiled Typst PDF preview right).

mod editor;
mod list;
mod preview;

pub use editor::DocumentEditorPanel;
pub use list::DocumentsListPanel;

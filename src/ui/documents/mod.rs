//! The Documents feature: a list of authored documents and a split-pane editor
//! (Typst/Markdown source left, compiled PDF preview right).

mod code_editor;
mod editor;
mod list;
mod preview;

pub use editor::DocumentEditorPanel;
pub use list::DocumentsListPanel;

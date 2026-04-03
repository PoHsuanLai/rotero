pub mod renderer;
pub mod annotations;
pub mod text_extract;

pub use renderer::{BookmarkEntry, ExtractedAnnotation, PdfDocumentInfo, PdfEngine, PdfError, RenderedPage};
pub use text_extract::{PageTextData, PdfDocMetadata, SearchMatch, TextSegment, group_into_lines};
pub use annotations::write_annotations;

pub mod annotations;
pub mod renderer;
pub mod text_extract;

pub use annotations::write_annotations;
pub use renderer::{
    BookmarkEntry, ExtractedAnnotation, PdfDocumentInfo, PdfEngine, PdfError, RenderFormat,
    RenderedPage,
};
pub use text_extract::{PageTextData, PdfDocMetadata, SearchMatch, TextSegment, group_into_lines};

//! A minimal [`World`] implementation backed by `typst-kit`.
//!
//! The main source is held in memory; every other [`FileId`] (crucially
//! `@preview/...` Universe packages) is resolved through a [`FileStore`] that
//! downloads and caches packages on first access.

use typst::diag::{FileResult, Warned};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

use typst_kit::datetime::Time;
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileStore, FsRoot, SystemFiles};
use typst_kit::fonts::{self, FontStore};
use typst_kit::packages::SystemPackages;

use crate::error::TypesetError;

/// User-Agent sent when downloading packages from Typst Universe.
const USER_AGENT: &str = concat!("rotero/", env!("CARGO_PKG_VERSION"), " (typst-embed)");

/// A compile session: one in-memory main source plus package/font resolution.
pub struct RoteroWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main_id: FileId,
    main_source: Source,
    files: FileStore<SystemFiles>,
    time: Time,
}

impl RoteroWorld {
    /// Build a world for `main_text`. `project_root` anchors relative imports in
    /// the source; a temp dir is fine when the source only imports `@preview/*`.
    pub fn new(main_text: &str, project_root: std::path::PathBuf) -> Self {
        let library = LazyHash::new(Library::default());

        let mut fonts = FontStore::new();
        fonts.extend(fonts::embedded());

        let vpath = VirtualPath::new("main.typ").expect("main.typ is a valid virtual path");
        let main_id = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let main_source = Source::new(main_id, main_text.to_string());

        let downloader = SystemDownloader::new(USER_AGENT);
        let packages = SystemPackages::new(downloader);
        let project = FsRoot::new(project_root);
        let files = FileStore::new(SystemFiles::new(project, packages));

        Self {
            library,
            fonts,
            main_id,
            main_source,
            files,
            time: Time::system(),
        }
    }

    /// The [`FileId`] of the in-memory main source (used to filter diagnostics
    /// to the authored document rather than imported packages).
    pub fn main_id(&self) -> FileId {
        self.main_id
    }

    /// Compile to PDF bytes, surfacing the first diagnostics on failure.
    pub fn compile_pdf(&self) -> Result<Vec<u8>, TypesetError> {
        let Warned { output, warnings: _ } =
            typst::compile::<typst_layout::PagedDocument>(self);
        let document = output.map_err(TypesetError::from_diagnostics)?;
        typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default())
            .map_err(TypesetError::from_diagnostics)
    }
}

impl World for RoteroWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main_id {
            Ok(self.main_source.clone())
        } else {
            self.files.source(id)
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main_id {
            Ok(Bytes::from_string(self.main_source.text().to_owned()))
        } else {
            self.files.file(id)
        }
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

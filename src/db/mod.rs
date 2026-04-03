pub mod annotations;
pub mod collections;
pub mod notes;
pub mod papers;
pub mod saved_searches;
pub mod schema;
pub mod tags;

use std::path::{Path, PathBuf};

use turso::Connection;

use crate::sync::engine::SyncConfig;

#[derive(Clone)]
pub struct Database {
    conn: Connection,
    data_dir: PathBuf,
}

impl PartialEq for Database {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Database {
    pub async fn init() -> Result<Self, String> {
        let config = SyncConfig::load();
        let data_dir = config.effective_library_path();

        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir: {e}"))?;

        // Create papers/ root and unsorted/
        let papers_dir = data_dir.join("papers");
        std::fs::create_dir_all(papers_dir.join("unsorted"))
            .map_err(|e| format!("Failed to create papers dir: {e}"))?;

        let db_path = data_dir.join("rotero.db");
        let db_path_str = db_path.to_string_lossy().to_string();

        let db = turso::Builder::new_local(&db_path_str)
            .experimental_index_method(true)
            .build()
            .await
            .map_err(|e| format!("Failed to open database: {e}"))?;

        let conn = db
            .connect()
            .map_err(|e| format!("Failed to connect: {e}"))?;

        schema::initialize_db(&conn)
            .await
            .map_err(|e| format!("Failed to initialize schema: {e}"))?;

        Ok(Self { conn, data_dir })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    #[allow(dead_code)]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Root directory for all paper PDFs.
    pub fn papers_dir(&self) -> PathBuf {
        self.data_dir.join("papers")
    }

    /// Resolve a relative pdf_path to an absolute path.
    pub fn resolve_pdf_path(&self, rel_path: &str) -> PathBuf {
        self.papers_dir().join(rel_path)
    }

    /// Import a PDF into the library with a clean, browsable path.
    ///
    /// Layout: `papers/{year}/{Title} - {FirstAuthor}.pdf`
    /// Falls back to `papers/unsorted/{original_filename}.pdf` if no metadata.
    pub fn import_pdf(
        &self,
        source_path: &str,
        title: Option<&str>,
        first_author: Option<&str>,
        year: Option<i32>,
    ) -> Result<String, String> {
        let source = Path::new(source_path);

        // Build clean filename
        let clean_name = build_clean_filename(source, title, first_author);

        // Build subfolder: year or "unsorted"
        let subfolder = match year {
            Some(y) => y.to_string(),
            None => "unsorted".to_string(),
        };

        let rel_dir = Path::new(&subfolder);
        let abs_dir = self.papers_dir().join(rel_dir);
        std::fs::create_dir_all(&abs_dir).map_err(|e| format!("Failed to create folder: {e}"))?;

        // Handle filename collisions
        let mut dest_name = clean_name.clone();
        let mut dest = abs_dir.join(&dest_name);
        let mut counter = 1;
        while dest.exists() {
            let stem = Path::new(&clean_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            dest_name = format!("{stem} ({counter}).pdf");
            dest = abs_dir.join(&dest_name);
            counter += 1;
        }

        std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy PDF: {e}"))?;

        // Return path relative to papers_dir
        let rel_path = format!("{subfolder}/{dest_name}");
        Ok(rel_path)
    }

    /// Import a PDF from bytes (e.g. downloaded from the web).
    /// Returns the relative path within the papers directory.
    pub fn import_pdf_bytes(
        &self,
        bytes: &[u8],
        title: &str,
        first_author: Option<&str>,
        year: Option<i32>,
    ) -> Result<String, String> {
        // Validate PDF header
        if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
            return Err("Not a valid PDF file".to_string());
        }

        let dummy_source = Path::new("download.pdf");
        let clean_name = build_clean_filename(dummy_source, Some(title), first_author);

        let subfolder = match year {
            Some(y) => y.to_string(),
            None => "unsorted".to_string(),
        };

        let rel_dir = Path::new(&subfolder);
        let abs_dir = self.papers_dir().join(rel_dir);
        std::fs::create_dir_all(&abs_dir).map_err(|e| format!("Failed to create folder: {e}"))?;

        let mut dest_name = clean_name.clone();
        let mut dest = abs_dir.join(&dest_name);
        let mut counter = 1;
        while dest.exists() {
            let stem = Path::new(&clean_name)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy();
            dest_name = format!("{stem} ({counter}).pdf");
            dest = abs_dir.join(&dest_name);
            counter += 1;
        }

        std::fs::write(&dest, bytes).map_err(|e| format!("Failed to write PDF: {e}"))?;

        let rel_path = format!("{subfolder}/{dest_name}");
        Ok(rel_path)
    }
}

/// Build a clean, human-readable filename from metadata.
/// Format: "Title - Author.pdf" or falls back to original filename.
fn build_clean_filename(source: &Path, title: Option<&str>, first_author: Option<&str>) -> String {
    let original = source
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "paper".to_string());

    match (title, first_author) {
        (Some(t), Some(a)) => {
            let clean_title = sanitize_filename(t, 80);
            let clean_author = sanitize_filename(a, 40);
            format!("{clean_title} - {clean_author}.pdf")
        }
        (Some(t), None) => {
            let clean_title = sanitize_filename(t, 100);
            format!("{clean_title}.pdf")
        }
        _ => {
            // Use original filename but ensure .pdf extension
            let clean = sanitize_filename(&original, 100);
            format!("{clean}.pdf")
        }
    }
}

/// Remove characters that are problematic in filenames, truncate to max length.
fn sanitize_filename(s: &str, max_len: usize) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            '\n' | '\r' | '\t' => ' ',
            _ => c,
        })
        .collect();

    let trimmed = cleaned.trim();

    if trimmed.len() <= max_len {
        trimmed.to_string()
    } else {
        // Truncate at word boundary
        let truncated = &trimmed[..max_len];
        match truncated.rfind(' ') {
            Some(pos) if pos > max_len / 2 => truncated[..pos].to_string(),
            _ => truncated.to_string(),
        }
    }
}

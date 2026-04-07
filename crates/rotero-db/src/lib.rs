pub mod annotations;
pub mod collections;
pub mod crr;
pub mod graph;
pub mod notes;
pub mod papers;
pub mod saved_searches;
pub mod schema;
pub mod sync_test_helpers;
pub mod tags;

pub use rotero_models::queries;

// Re-export so the app crate doesn't need a direct turso dependency.
pub use turso;

use std::path::{Path, PathBuf};

use turso::Connection;

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
    /// Open (or create) the database at the given library directory.
    pub async fn open(data_dir: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data dir: {e}"))?;

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

    pub fn from_conn(conn: Connection, data_dir: PathBuf) -> Self {
        Self { conn, data_dir }
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn papers_dir(&self) -> PathBuf {
        self.data_dir.join("papers")
    }

    pub fn resolve_pdf_path(&self, rel_path: &str) -> PathBuf {
        self.papers_dir().join(rel_path)
    }

    /// Import a PDF into the library.
    /// Layout: `papers/{year}/{Title} - {FirstAuthor}.pdf`, falling back to `papers/unsorted/`.
    pub fn import_pdf(
        &self,
        source_path: &str,
        title: Option<&str>,
        first_author: Option<&str>,
        year: Option<i32>,
    ) -> Result<String, String> {
        let source = Path::new(source_path);

        let clean_name = build_clean_filename(source, title, first_author);

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

        std::fs::copy(source, &dest).map_err(|e| format!("Failed to copy PDF: {e}"))?;

        let rel_path = std::path::Path::new(&subfolder)
            .join(&dest_name)
            .to_string_lossy()
            .into_owned();
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

        let rel_path = std::path::Path::new(&subfolder)
            .join(&dest_name)
            .to_string_lossy()
            .into_owned();
        Ok(rel_path)
    }
}

/// Format: "Title - Author.pdf", falling back to original filename.
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
            // Fall back to original filename
            let clean = sanitize_filename(&original, 100);
            format!("{clean}.pdf")
        }
    }
}

/// Remove filesystem-unsafe characters and truncate to `max_len`.
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

# Rotero

A lightweight, Rust-native paper reading and reference management app. Built as a faster, simpler alternative to Zotero.

## Features

- **PDF Viewer** — Open and read PDFs with zoom controls
- **Library Management** — Organize papers into collections with tags
- **DOI Metadata Fetch** — Auto-populate paper details from CrossRef
- **Browser Connector** — Save papers from your browser with one click (Chrome extension)
- **SQLite Storage** — Fast local database, no server needed

### Planned

- PDF annotations (highlights, sticky notes)
- Full-text search across papers and PDFs
- BibTeX / RIS / CSL JSON import and export
- Citation and bibliography generation (APA, IEEE, Chicago, etc.)
- Cross-device sync via cloud folders or WebDAV

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2024)
- [just](https://github.com/casey/just) — task runner

```sh
# Install just (if not already installed)
cargo install just
```

### Build & Run

```sh
git clone https://github.com/your-username/rotero.git
cd rotero

# Download PDFium, build, and run
just run
```

The `just run` command automatically:
1. Downloads the PDFium rendering library for your platform (macOS arm64/x64, Linux arm64/x64)
2. Builds the project
3. Runs the app with the correct library path

### Other Commands

```sh
just check          # Check all crates compile
just lint           # Run clippy
just build-release  # Build in release mode
just run-release    # Run in release mode
just clean          # Clean build artifacts
just clean-all      # Clean build + PDFium binary
```

## Browser Extension

The Chrome extension lets you save papers directly from publisher sites, arXiv, Google Scholar, and more.

### Install

1. Open Chrome → `chrome://extensions/`
2. Enable "Developer mode"
3. Click "Load unpacked" → select the `extension/` folder
4. Make sure Rotero is running (the extension connects to `localhost:21984`)

### Supported Sites

arXiv, DOI.org, Google Scholar, PubMed, Semantic Scholar, ScienceDirect, Springer, Nature, Wiley, IEEE, ACM — and any page with standard citation meta tags.

### Test the API

```sh
# Check if the connector is running
just test-connector

# Save a test paper
just test-save-paper
```

## Architecture

Cargo workspace with 6 crates optimized for fast incremental compilation:

```
rotero-models       ← shared data types (Paper, Collection, Tag, etc.)
    ↑
    ├── rotero-pdf         pdfium-render, lopdf
    ├── rotero-search      tantivy
    ├── rotero-bib         hayagriva, biblatex
    ├── rotero-connector   axum (HTTP server)
    │
    └── rotero (app)       dioxus, rusqlite, reqwest
```

Editing a UI component only recompiles the app crate (~8-12s), not the PDF/search/citation crates.

## Tech Stack

| Component | Crate |
|---|---|
| UI | dioxus (desktop/WebView) |
| PDF rendering | pdfium-render |
| Database | rusqlite (SQLite, bundled) |
| HTTP client | reqwest |
| Browser connector | axum |
| Serialization | serde |

## Data Storage

Papers and metadata are stored in a local SQLite database. Imported PDFs are copied to a managed directory.

| Platform | Location |
|---|---|
| macOS | `~/Library/Application Support/com.rotero.Rotero/` |
| Linux | `~/.local/share/rotero/` |

## License

MIT

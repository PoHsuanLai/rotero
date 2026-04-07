# Rotero

<p align="center">
  <img src="assets/icon.png" alt="Rotero" width="128" />
</p>

A lightweight, Rust-native paper reading and reference management app. Built as a faster, simpler alternative to Zotero.

## Features

- **PDF Viewer** — Read PDFs with page navigation and zoom controls
- **PDF Annotations** — Highlights and sticky notes on PDFs
- **Library Management** — Organize papers into collections with tags
- **Full-Text Search** — Search across papers and PDFs (built-in FTS)
- **DOI Metadata Fetch** — Auto-populate paper details from CrossRef, Semantic Scholar, OpenAlex
- **Zotero Web Translators** — 742 community-maintained scrapers for Google Scholar, arXiv, Nature, IEEE, ACM, PubMed, and more
- **BibTeX / RIS / CSL Import & Export** — Interchange with other reference managers
- **Citation Generation** — Generate bibliographies in 14 CSL styles (APA, IEEE, Chicago, etc.)
- **Browser Connector** — Save papers from your browser with one click (Chrome extension)
- **Citation Graph** — Visualize paper relationships
- **Cross-Device Sync** — File-based sync via cloud folders
- **SQLite Storage** — Fast local database, no server needed

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/)
- [just](https://github.com/casey/just)

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

## Performance

Memory usage with 5 PDF tabs open (average of 5 runs, macOS):

| | Rotero | Zotero 7 |
|---|---|---|
| Memory | ~220 MB | ~1.4 GB |

## Architecture

Cargo workspace with 9 crates:

| Crate | Purpose | Key deps |
|---|---|---|
| `rotero-models` | Shared data types (Paper, Collection, Tag, Annotation, Note) | serde |
| `rotero-db` | SQLite CRUD operations | turso |
| `rotero-pdf` | PDF rendering + annotation writing | pdfium-render, lopdf |
| `rotero-bib` | BibTeX/RIS/CSL import/export + citation generation | biblatex, hayagriva |
| `rotero-connector` | Browser extension HTTP server | axum |
| `rotero-translate` | Zotero translation server (Node.js sidecar) | reqwest |
| `rotero-graph` | Citation graph visualization | — |
| `rotero-mcp` | MCP server for AI agent integration | rmcp |
| `rotero` (app) | Desktop UI, metadata fetching, state management | dioxus, reqwest |

All library crates depend on `rotero-models`. The app crate depends on all of them.

## Tech Stack

| Component | Crate |
|---|---|
| UI | dioxus (desktop/WebView) |
| PDF rendering | pdfium-render |
| PDF annotation | lopdf |
| Database | turso (pure Rust SQLite) |
| Full-text search | turso FTS |
| Citations | hayagriva (CSL) |
| Web scraping | Zotero translation-server (Node.js) |
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

AGPL-3.0-or-later

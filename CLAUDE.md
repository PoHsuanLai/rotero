# Rotero

Lightweight Rust paper reading app replacing Zotero. Uses Dioxus for desktop UI.

## Architecture

Cargo workspace with 5 library crates + 1 app crate:

- `rotero-models` — shared data types (Paper, Collection, Tag, Annotation, Note)
- `rotero-pdf` — PDF rendering (pdfium-render) and annotation writing (lopdf)
- `rotero-search` — removed (turso handles FTS natively)
- `rotero-bib` — BibTeX/RIS/CSL import/export (biblatex, hayagriva) — Phase 4-5
- `rotero-connector` — browser extension HTTP server (axum, port 21984)
- `rotero` (app) — Dioxus desktop UI, SQLite DB, metadata fetching, state management

Dependency flow: `rotero-models` ← all library crates ← `rotero` (app). No cycles.

## Build & Run

Requires [just](https://github.com/casey/just) for task running.

```sh
just run          # downloads PDFium, builds, runs (debug)
just run-release  # same but release mode
just check        # cargo check --workspace
just lint         # clippy
```

PDFium binary is auto-downloaded to `lib/` by the justfile. Set `PDFIUM_DYNAMIC_LIB_PATH` if placing it elsewhere.

## Key Paths

- SQLite DB: `~/Library/Application Support/com.rotero.Rotero/rotero.db` (macOS)
- Imported PDFs: `~/Library/Application Support/com.rotero.Rotero/pdfs/`
- Browser connector: `http://127.0.0.1:21984`

## Code Layout

- `src/ui/` — Dioxus components. Layout → Sidebar + (LibraryPanel | PdfViewer)
- `src/db/` — rusqlite CRUD. Each file maps to one table.
- `src/state/` — Dioxus signals (`app_state.rs`) and command dispatchers (`commands.rs`)
- `src/metadata/` — CrossRef API client for DOI lookups
- `extension/` — Chrome manifest v3 browser extension

## Conventions

- Models are plain structs with serde derives, no business logic
- All SQLite access goes through `src/db/`. No raw SQL elsewhere.
- DB uses turso (pure Rust SQLite) — Connection is Clone+Send+Sync, no Arc<Mutex<>> needed
- All DB operations are async — UI uses `spawn()` for DB calls
- FTS search built into turso via `CREATE INDEX ... USING fts`
- PDF pages are rendered to base64 PNG and displayed as `<img>` tags in the WebView
- The browser connector runs in a background thread with its own tokio runtime
- Use `use_context::<Database>()` in components to access the DB
- Use `use_context::<Signal<LibraryState>>()` for reactive library state

## Phased Roadmap

1. ~~Core scaffold + PDF viewer~~ (done)
2. ~~Library management + metadata + browser connector~~ (done)
3. ~~PDF annotations (highlights, notes)~~ (done)
4. ~~Search + BibTeX import/export~~ (done — turso FTS, no separate tantivy)
5. ~~Citation generation (hayagriva CSL, 14 styles)~~ (done)
6. ~~Sync (file-based via cloud folder)~~ (done)

# Rotero

<p align="center">
  <img src="assets/icon.png" alt="Rotero" width="128" />
</p>

<p align="center">
  <a href="https://github.com/PoHsuanLai/rotero/actions/workflows/ci.yml"><img src="https://github.com/PoHsuanLai/rotero/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/PoHsuanLai/rotero/releases/latest"><img src="https://img.shields.io/github/v/release/PoHsuanLai/rotero" alt="Release"></a>
  <a href="https://github.com/PoHsuanLai/rotero/blob/master/LICENSE"><img src="https://img.shields.io/github/license/PoHsuanLai/rotero" alt="License"></a>
  <img src="https://img.shields.io/badge/rust-1.93+-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/dioxus-0.7-blue" alt="Dioxus">
  <img src="https://img.shields.io/badge/turso-0.6-teal" alt="Turso">
</p>

A fast, private, local-first reference manager built with Rust. Read, annotate, cite, and explore your papers — without the bloat.

<p align="center">
  <img src="assets/screenshot-reader.png" alt="Rotero PDF Reader" width="800" />
</p>

## Why Rotero

- **Native Rust, no Electron** — Single binary, starts instantly, stays light
- **742 Zotero web translators** — One-click import from Google Scholar, arXiv, PubMed, and 40+ academic sites
- **Citation graph** — Interactive visualization of how your papers connect
- **AI research assistant** — Chat with your papers via ACP — use your Claude subscription, no API costs
- **CRR sync** — Custom conflict-free replicated relations for multi-device sync
- **Local-first** — SQLite database, no accounts, no telemetry, no cloud dependency

## Performance

Memory with 5 PDF tabs open (avg of 5 runs, macOS):

| | Rotero | Zotero 7 |
|---|---|---|
| Memory | ~220 MB | ~1.4 GB |

## Status

Under active development. Known limitations:

- PDF virtual text layer (selection/copy) needs refinement
- Mobile app (iOS/Android) planned, not yet available

## Getting Started

Requires [Rust](https://rustup.rs/) and [just](https://github.com/casey/just).

```sh
git clone https://github.com/PoHsuanLai/rotero.git
cd rotero
just run    # downloads PDFium, builds, runs
```

Other commands: `just check`, `just lint`, `just build-release`, `just run-release`, `just clean`

## Browser Extension

1. `chrome://extensions/` → Developer mode → Load unpacked → select `extension/`
2. Keep Rotero running (connects to `localhost:21984`)

## Architecture

Cargo workspace with 9 crates:

| Crate | Purpose | Key deps |
|---|---|---|
| `rotero-models` | Shared data types | serde |
| `rotero-db` | SQLite CRUD | turso |
| `rotero-pdf` | PDF rendering + annotation writing | pdfium-render, lopdf |
| `rotero-bib` | BibTeX/RIS/CSL + citation generation | biblatex, hayagriva |
| `rotero-connector` | Browser extension HTTP server | axum |
| `rotero-translate` | Zotero translation server (Node.js sidecar) | reqwest |
| `rotero-graph` | Citation graph visualization | fdg |
| `rotero-mcp` | MCP server for AI integration | rmcp |
| `rotero` (app) | Desktop UI, state management | dioxus, reqwest |

## Data Storage

Local SQLite database. PDFs copied to a managed directory.

| Platform | Location |
|---|---|
| macOS | `~/Library/Application Support/com.rotero.Rotero/` |
| Linux | `~/.local/share/rotero/` |

## License

AGPL-3.0-or-later

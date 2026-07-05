# Document editor bundle

`assets/editor.js` is a pre-built [CodeMirror 6](https://codemirror.net) bundle
that powers the Documents editor (Typst + Markdown syntax highlighting). It is
vendored so the app builds fully offline with no Node toolchain in the Rust
build — only the built `editor.js` is loaded at runtime (via `include_str!` in
`src/app/mod.rs`).

The bundle exposes `window.__roteroEditor` and pushes edits onto
`window.__roteroEditorEvents`, mirroring the graph view's JS bridge. The Rust
side is `src/ui/documents/code_editor.rs`.

## Rebuild

Sources here are the input; the output is `../editor.js`.

```sh
cd assets/editor-src
npm install
npx esbuild editor.js \
  --bundle --format=iife --platform=browser --target=es2020 \
  --minify --legal-comments=none \
  --outfile=../editor.js
```

Keep the CodeMirror dependency versions pinned in `package.json`; bump
deliberately and re-run the build. The Typst mode is a hand-written
`StreamLanguage` tokenizer (Typst has no official Lezer grammar); a real
language server (tinymist) is a future addition.

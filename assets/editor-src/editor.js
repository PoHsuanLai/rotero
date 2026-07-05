// Rotero document editor: a thin CodeMirror 6 wrapper exposed as
// `window.__roteroEditor`, mirroring the `window.__roteroGraph` bridge pattern.
//
// The Rust side (src/ui/documents/code_editor.rs) drives it through
// `document::eval` and receives edits by polling `window.__roteroEditorEvents`.

import { EditorState, Compartment } from "@codemirror/state";
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  rectangularSelection,
} from "@codemirror/view";
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from "@codemirror/commands";
import {
  bracketMatching,
  indentOnInput,
  foldGutter,
  foldKeymap,
  syntaxHighlighting,
  HighlightStyle,
  StreamLanguage,
} from "@codemirror/language";
import { closeBrackets, closeBracketsKeymap } from "@codemirror/autocomplete";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { markdown } from "@codemirror/lang-markdown";
import { tags as t } from "@lezer/highlight";

// --- Typst syntax mode --------------------------------------------------
// Typst has no official Lezer grammar in CodeMirror, so we hand-roll a
// StreamLanguage tokenizer. It covers the constructs a paper author sees most:
// headings, code exprs (`#...`), math (`$...$`), strings, comments, labels/refs
// (`@key`, `<label>`), and markup emphasis. Good enough for readable coloring;
// the real language server (tinymist) is a later addition.
const typstLanguage = StreamLanguage.define({
  name: "typst",
  startState() {
    return { inMath: false };
  },
  token(stream, state) {
    // Inside a math block: consume until the closing `$`.
    if (state.inMath) {
      if (stream.match(/^[^$]+/)) return "atom";
      if (stream.eat("$")) {
        state.inMath = false;
        return "keyword";
      }
      stream.next();
      return null;
    }

    // Line comment.
    if (stream.match(/^\/\/.*/)) return "comment";
    // Block comment (single-line best-effort).
    if (stream.match(/^\/\*.*?\*\//)) return "comment";

    // Heading: `=`, `==`, ... at line start.
    if (stream.sol() && stream.match(/^=+\s/)) return "heading";

    // List / term markers at line start.
    if (stream.sol() && stream.match(/^\s*[-+]\s/)) return "list";
    if (stream.sol() && stream.match(/^\s*\/\s.*?:/)) return "list";

    // Enter math mode.
    if (stream.eat("$")) {
      state.inMath = true;
      return "keyword";
    }

    // String literal.
    if (stream.match(/^"(?:[^"\\]|\\.)*"/)) return "string";

    // Code expression / function call / keyword: `#...`.
    if (stream.eat("#")) {
      stream.match(/^[A-Za-z_][A-Za-z0-9_.-]*/);
      return "keyword";
    }

    // Reference / citation: `@key`.
    if (stream.match(/^@[A-Za-z0-9_:.-]+/)) return "link";
    // Label: `<name>`.
    if (stream.match(/^<[A-Za-z0-9_:.-]+>/)) return "labelName";

    // Emphasis markers.
    if (stream.eat("*")) return "strong";
    if (stream.eat("_")) return "emphasis";
    // Raw / code span.
    if (stream.match(/^`[^`]*`/)) return "monospace";

    // Escapes.
    if (stream.match(/^\\./)) return "escape";

    stream.next();
    return null;
  },
});

// Map our token names to concrete colours via CSS variables so the editor
// tracks the app theme (light/dark) automatically.
const cssVar = (name, fallback) => `var(${name}, ${fallback})`;

const highlightStyle = HighlightStyle.define([
  { tag: t.heading, color: cssVar("--accent-primary", "#0d9488"), fontWeight: "600" },
  { tag: t.keyword, color: cssVar("--accent-primary", "#0d9488") },
  { tag: t.string, color: cssVar("--syntax-string", "#0a7d4f") },
  { tag: t.comment, color: cssVar("--text-tertiary", "#8a8a8a"), fontStyle: "italic" },
  { tag: t.atom, color: cssVar("--syntax-math", "#7c5cff") },
  { tag: [t.link, t.labelName], color: cssVar("--syntax-cite", "#b5651d") },
  { tag: t.list, color: cssVar("--accent-primary", "#0d9488") },
  { tag: t.strong, fontWeight: "700" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.monospace, color: cssVar("--syntax-string", "#0a7d4f") },
  { tag: t.escape, color: cssVar("--text-tertiary", "#8a8a8a") },
]);

// Editor chrome pulls entirely from the app's CSS tokens so it never clashes
// with the surrounding UI or the theme toggle.
const theme = EditorView.theme({
  "&": {
    height: "100%",
    fontSize: "var(--text-sm, 13px)",
    color: "var(--text-primary, #1a1a1a)",
    backgroundColor: "var(--bg-surface, #fff)",
  },
  ".cm-content": {
    fontFamily: "var(--font-mono, ui-monospace, monospace)",
    lineHeight: "1.6",
    padding: "16px 0",
    caretColor: "var(--accent-primary, #0d9488)",
  },
  ".cm-scroller": { overflow: "auto" },
  ".cm-gutters": {
    backgroundColor: "transparent",
    color: "var(--text-tertiary, #999)",
    border: "none",
  },
  ".cm-activeLine": { backgroundColor: "var(--bg-sidebar-hover, rgba(0,0,0,0.03))" },
  ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text-secondary, #666)" },
  "&.cm-focused": { outline: "none" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "var(--accent-ring, rgba(13,148,136,0.25))",
  },
  ".cm-matchingBracket": {
    backgroundColor: "var(--accent-ring, rgba(13,148,136,0.25))",
    outline: "1px solid var(--accent-primary, #0d9488)",
  },
});

function languageFor(lang) {
  if (lang === "markdown") return markdown();
  return typstLanguage; // default: Typst
}

const languageConf = new Compartment();

const baseExtensions = () => [
  lineNumbers(),
  highlightActiveLineGutter(),
  foldGutter(),
  history(),
  drawSelection(),
  rectangularSelection(),
  indentOnInput(),
  bracketMatching(),
  closeBrackets(),
  highlightActiveLine(),
  highlightSelectionMatches(),
  syntaxHighlighting(highlightStyle),
  theme,
  EditorView.lineWrapping,
  keymap.of([
    ...closeBracketsKeymap,
    ...defaultKeymap,
    ...searchKeymap,
    ...historyKeymap,
    ...foldKeymap,
    indentWithTab,
  ]),
];

// --- Bridge -------------------------------------------------------------
// One editor instance per mount id. Edits are pushed onto a queue the Rust
// side polls (same mechanism as the graph view's event queue).
const editors = new Map();
window.__roteroEditorEvents = [];

function pushEvent(ev) {
  window.__roteroEditorEvents.push(ev);
}

window.__roteroEditor = {
  // Create (or recreate) an editor in the element with the given id.
  mount(id, doc, lang) {
    const el = document.getElementById(id);
    if (!el) return false;
    // Tear down a previous instance bound to the same element.
    const prev = editors.get(id);
    if (prev) {
      prev.destroy();
      editors.delete(id);
    }
    el.innerHTML = "";

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        pushEvent({ id, type: "change", value: update.state.doc.toString() });
      }
    });

    const state = EditorState.create({
      doc: doc || "",
      extensions: [
        ...baseExtensions(),
        languageConf.of(languageFor(lang)),
        updateListener,
      ],
    });
    const view = new EditorView({ state, parent: el });
    editors.set(id, view);
    return true;
  },

  // Replace the whole document (e.g. live agent authoring writes a new body).
  // No-op if the incoming text already matches, so we don't fight the cursor.
  setDoc(id, doc) {
    const view = editors.get(id);
    if (!view) return;
    const current = view.state.doc.toString();
    if (current === doc) return;
    view.dispatch({
      changes: { from: 0, to: current.length, insert: doc || "" },
    });
  },

  // Switch the active language mode without losing content.
  setLanguage(id, lang) {
    const view = editors.get(id);
    if (!view) return;
    view.dispatch({ effects: languageConf.reconfigure(languageFor(lang)) });
  },

  focus(id) {
    const view = editors.get(id);
    if (view) view.focus();
  },

  unmount(id) {
    const view = editors.get(id);
    if (view) {
      view.destroy();
      editors.delete(id);
    }
  },
};

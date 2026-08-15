---
layout: docs
title: Annotations and notes
description: Highlight, underline, draw on, and comment on a PDF, then extract everything you marked into a single note attached to the paper.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Rotero has five annotation types, all on the [reader]({base}/docs/reader)
toolbar:

- **Highlight** — colored background over selected text
- **Underline** — a rule under selected text
- **Sticky Note** — a marker you attach to a spot on the page, with a comment
- **Draw** — freehand ink, for diagrams and margins
- **Text** — typed text placed directly on the page

Pick a tool and a row of six color swatches appears next to it. The color
applies to what you make next; you can recolor an existing annotation later from
its right-click menu.

<Figure src="pdf-annotations.png" alt="A PDF page with highlights and a sticky note, alongside the annotation panel listing them." caption="Annotations on the page and the panel listing them, grouped by page." />

## Undo and redo

The toolbar's **Undo** and **Redo** cover creating, deleting, moving, and
editing an annotation — not only the last stroke. If you drag a highlight to the
wrong place, undo puts it back.

## The annotation panel

**Notes** on the toolbar (or `N`) opens the panel; **Hide Notes** (`N` again)
closes it. The header counts what the document holds — `Annotations (23)`.

You can edit an annotation's note inline in the panel and click **Save**.

Right-click an entry for:

| Action | What it does |
| --- | --- |
| Go to page N | Scrolls the document to that annotation |
| Edit note | Opens the inline editor |
| Copy text | Copies the annotated text to the clipboard |
| Color swatches | Recolors that annotation |
| Delete | Removes it |

## Extract to Note

**Extract to Note** in the panel collects everything in the document into a
single markdown note attached to the paper. Entries are grouped by page and
labeled by kind — Highlight, Note, Area, Underline, Ink, Text — so a reading
pass turns into something you can read end to end without the PDF open.

Run it again after more reading and you get a fresh note reflecting the current
state of the document.

## Notes on a paper

Notes appear in the paper detail panel under a `Notes (N)` heading. They come
from three places: annotation extraction, the
[AI assistant]({base}/docs/ai-assistant), and the
[MCP server]({base}/docs/mcp).

Notes render as markdown with LaTeX support, so `$\alpha$` and display equations
come out typeset rather than as source. Each note can be deleted individually.

## Getting annotations out of Rotero

**Export PDF** on the reader toolbar writes your annotations into the file
itself, producing a flattened `<name>-annotated.pdf` next to the original. The
button only appears once the document has annotations.

<Callout type="note">

The exported copy is flattened — the annotations are part of the page, so they
render the same in every viewer, but they are no longer separately editable in
that copy. Your originals stay editable in Rotero.

</Callout>

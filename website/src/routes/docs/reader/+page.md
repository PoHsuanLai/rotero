---
layout: docs
title: The PDF reader
description: Open PDFs in tabs, move through a document with the outline and thumbnails, search inside it, and set the zoom level.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
  import Pin from '$lib/components/docs/Pin.svelte';
</script>

There are two ways into the reader. The **Open PDF** button in the sidebar
(`⌘O`) opens any PDF on disk — it does not have to be in your library. The
**Open** button on a paper row opens that paper's attached PDF.

Before you have opened anything, the reader shows "Open a PDF to get started".

<Figure src="pdf-reader.png" alt="The Rotero PDF reader with a paper open, showing the tab bar, toolbar, and page content." caption="A document open in the reader. The toolbar runs along the top; tabs sit above it.">
  <Pin n={1} x={18} y={7}>Tabs</Pin>
  <Pin n={2} x={52} y={17}>Annotation tools</Pin>
  <Pin n={3} x={88} y={17} side="left">Zoom</Pin>
</Figure>

## Tabs

Each PDF you open gets a tab, and each tab has a close **x**. Rotero remembers
the scroll position and last page per tab, so switching between two papers and
back puts you where you left off.

Right-click a tab for **Close**, **Close other tabs**, **Close tabs to the
right**, and **Show in library**, which selects the paper the PDF belongs to.

## Reading the document

Pages scroll continuously. Rotero renders a sliding window of pages around the
one you are looking at rather than the whole file, so a 400-page thesis opens as
fast as a short paper.

The toolbar, left to right:

| Control | What it does |
| --- | --- |
| Page count | Where you are in the document |
| Highlight, Underline, Sticky Note, Draw, Text | The [annotation tools]({base}/docs/annotations) |
| Color row | Six swatches, shown once a tool is active |
| Undo / Redo | Steps annotation edits back and forward |
| Pages | Thumbnail sidebar |
| TOC | The PDF's own bookmark outline |
| Find | Search inside the document (`⌘F`) |
| Notes / Hide Notes | The annotation panel (`N`) |
| Export PDF | Writes annotations into a flattened copy — appears only once the document has annotations |
| Zoom out / percentage / zoom in | 0.5x to 5.0x, in 0.3 steps |

**TOC** reads the bookmark outline the publisher put in the file. Papers that
were exported without one show nothing there — use **Pages** or **Find**
instead.

## Searching inside a document

Press `⌘F` or click **Find**. The box reads "Search in PDF…" and counts matches
as you type, showing your position as something like `3/17`. The up and down
arrows step through hits, `Enter` advances, and `Esc` closes the bar.

This searches the open document only. To search across every paper in your
library, use [Search]({base}/docs/search).

## Annotations already in the file

When Rotero opens a PDF it reads any annotations already embedded in the file —
highlights you made in Preview, comments a co-author left in Acrobat — and
imports them into your library. Reopening the same file does not duplicate them.

<Callout type="tip">

That import runs on any PDF you open, including one that is not in your library.
It is a quick way to see what is marked up in a file someone sent you.

</Callout>

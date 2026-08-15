---
layout: docs
title: A tour of the app
description: What each part of the Rotero window does — the sidebar, the paper list, selection and the context menu, and the detail panel on the right.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
  import Pin from '$lib/components/docs/Pin.svelte';
</script>

The window has three vertical parts: the sidebar on the left, the paper list in
the middle, and the detail panel on the right when a paper is selected. This
page walks through each one.

<Figure src="library-overview.png" alt="The Rotero window showing the sidebar, the paper list, and the detail panel." caption="The three panes of the main window.">
  <Pin n={1} x={10} y={20}>Sidebar</Pin>
  <Pin n={2} x={48} y={12}>Search</Pin>
  <Pin n={3} x={48} y={55}>Paper list</Pin>
  <Pin n={4} x={86} y={40} side="left">Detail panel</Pin>
</Figure>

## The sidebar

From top to bottom:

**Open PDF** opens a file straight into the reader.

**Library** holds five fixed views, each showing a live count: All Papers,
Recently Added, Favorites, Unread, and
[Duplicates]({base}/docs/duplicates). Clicking one filters the list in the
middle.

**Recent** lists the last five PDFs you opened. Right-click one and choose
**Show in library** to jump to its row in the list.

**Collections** is your folder structure, nestable as deep as you want.
**Tags** are colored chips. Both are covered in
[Collections and tags]({base}/docs/collections-tags).

**Saved Searches** collects searches you bookmarked from the search field. See
[Search]({base}/docs/search).

**Settings** sits at the bottom.

The whole sidebar collapses to an icon strip when you want the horizontal space
back.

<Figure src="sidebar-collections.png" alt="The sidebar with nested collections and colored tag chips." caption="Collections nest arbitrarily deep; tags show as colored chips." />

## The search field

One field, at the top of the library panel, with the placeholder "Search your
library and the web...". Type three or more characters and it searches your
local full text and three external sources at the same time, splitting the
results into **In your library** and **From the web**. `⌘L` puts the cursor in
it.

Next to it are the sort controls: Date Added, Date Modified, Title, Year, First
Author, or Citations, with a toggle for ascending or descending.

## The paper list

Each row is one paper. The list respects whatever the sidebar has selected —
a collection, a tag, Unread, a saved search — and whatever you have typed into
search.

### Selecting

Click a row to select it. `⌘`-click (`Ctrl`-click on Windows and Linux) adds or
removes individual rows. `Shift`-click selects a range. `⌘A` selects everything
in the current view.

`↑` and `↓` move the selection, `Enter` opens the selected paper, and
`Backspace` deletes it.

Select two or more papers and the detail panel turns into a summary with three
bulk actions: **Favorite All**, **Mark All Read**, and **Delete All**.

### The context menu

Right-click any row for the paper menu. It works on a single paper or on
everything you have selected:

- **Open PDF**
- **Download PDF** — shown when the paper has no local file but does have a URL
- **Favorite** / **Unfavorite**
- **Mark as read** / **Mark as unread**
- **Add to Collection** — picks a collection in place, without leaving the menu
- **Add Tag** — a picker with a "New tag..." input at the bottom
- **Copy DOI**
- **Remove from Collection** — only appears while you are viewing one
- **Delete** — asks for confirmation first

`⌘⇧F` toggles favorite and `⌘⇧U` toggles read on the selection without opening
the menu.

## The detail panel

Select exactly one paper and the right side shows everything Rotero knows about
it.

The metadata fields sit at the top. Below them:

**Citation key** — click it to edit inline, or use the copy button next to it.

**Collection** and **Tags** pickers assign the paper without dragging.

**Cite** opens the citation dialog, covered in
[Citation styles]({base}/docs/citation-styles).

**Notes** are your own text on the paper, separate from annotations in the PDF.

At the bottom are three actions:

- **Open Paper** — opens the PDF in the reader.
- **Find PDF** — searches open-access sources for a file. If that comes up
  empty, it offers **Ask Agent**, which hands the problem to the
  [AI assistant]({base}/docs/ai-assistant).
- **Delete Paper**.

<Callout type="tip">

Drag the panel's left edge to resize it. Rotero remembers the width you set.

</Callout>

## The reader

Opening a paper replaces the library panel with the PDF reader. The sidebar
stays where it is, so you can jump to another collection without closing the
document. `⌘1` returns to the library.

The reader has its own pages in this guide:
[The PDF reader]({base}/docs/reader),
[Annotations and notes]({base}/docs/annotations), and
[Following citations]({base}/docs/citations-in-pdfs).

## Menus and shortcuts

The File menu holds **Import BibTeX…** (`⌘I`) and export (`⌘E`). `⌘N` creates a
collection, `⌘F` opens find, and `⌘1` shows the library. The full table is in
[Keyboard shortcuts]({base}/docs/shortcuts).

---
layout: docs
title: First steps
description: Get your first paper into Rotero, let it fetch the metadata, open it in the reader, and make a highlight — about ten minutes end to end.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

This page takes you from a freshly installed Rotero to one paper in your
library, with metadata filled in and a highlight on the page. It assumes you
have already done [Install]({base}/docs/install).

## Launching the app

Open Rotero the way you open anything else — Applications on macOS, the Start
menu on Windows, your launcher on Linux. There is no account to create and no
sign-in step. The first launch creates a SQLite database and a `pdfs` folder on
your machine, and everything you add stays in those two places. See
[Where your data lives]({base}/docs/data-locations) if you want the exact paths.

## The empty state

A new library shows the sidebar on the left and an empty library panel in the
middle. Nothing is broken — there is just nothing to list yet.

The two things to notice before you add anything:

The **sidebar** groups your library into All Papers, Recently Added, Favorites,
Unread, and Duplicates, each with a live count. Below those sit Collections,
Tags, and Saved Searches, which fill in as you use them. The Settings button is
at the bottom.

The **search field** at the top of the library panel reads "Search your library
and the web...". It does both at once, which matters later — you can find a
paper you do not have yet from the same box you use to find one you do.

<Figure src="library-overview.png" alt="The Rotero window with the sidebar on the left, the paper list in the middle, and the detail panel on the right." caption="The main window. The sidebar collapses to an icon strip if you want the space." />

## Getting your first paper in

There are six ways to add papers, covered in
[Adding papers]({base}/docs/importing). For your first one, use whichever of
these two matches what you have in front of you.

### If you have a PDF on disk

Click **+ Add PDF** and pick the file. Rotero copies it into the library
folder, so the original stays where it is and you can move or delete it later
without breaking anything.

Rotero then reads the text of the PDF looking for a DOI. If it finds one, and
automatic metadata fetching is on, it queries CrossRef and fills in the title,
authors, journal, year, and abstract on its own. You will see the row in the
list change from a filename to a real citation a moment after the import.

You can also drag PDF files straight onto the library panel instead of using
the button.

### If you have a DOI

Click **+ DOI**, paste the identifier, and press **Fetch**. Rotero looks up the
metadata and then tries to find a legally free copy of the PDF through
open-access sources. When it succeeds you get the record and the file together;
when it does not, you get the record and can attach a PDF later.

<Callout type="tip">

A DOI works with or without the `https://doi.org/` prefix. Both `10.1145/3025453.3025912`
and the full URL are accepted.

</Callout>

## What auto-fetch actually does

Automatic metadata fetching is what turns a file called `paper_final_v3.pdf`
into a proper record. It runs in this order:

1. Extract the text of the first pages and look for a DOI.
2. Ask CrossRef for the record behind that DOI.
3. Write the title, authors, container, year, and abstract onto the paper.

If the PDF has no DOI in its text — common for preprints, scans, and older
papers — nothing is filled in and the entry keeps its filename as the title.
That is fixable: select the paper and edit the fields in the detail panel on the
right, or delete it and re-add it through **+ DOI** instead.

<Callout type="note">

Auto-fetch needs a network connection and can be turned off in Settings. With
it off, imports are entirely local and nothing leaves your machine.

</Callout>

## Opening it in the reader

Select the paper and press `Enter`, or double-click the row. The PDF opens in
Rotero's own reader — pages render in the app, so there is no round trip to
Preview or Acrobat.

Move through the document by scrolling, or with `↑` and `↓` when the list has
focus. `⌘1` takes you back to the library at any point, and the Recent section
of the sidebar keeps the last five PDFs you opened so you can get back quickly.

<Figure src="pdf-reader.png" alt="A PDF open in the Rotero reader with the page rendered in the center." caption="The reader. Full detail is in the reader page." />

## Making a highlight

Select some text on the page and choose a highlight color. The highlight is
saved to your library immediately and is written into the PDF file itself, so it
survives if you open the same file elsewhere.

Click an existing highlight to change its color, attach a comment to it, or
remove it. [Annotations and notes]({base}/docs/annotations) covers comments,
page notes, and how annotations are exported.

<Figure src="pdf-annotations.png" alt="A page with several colored highlights and a comment attached to one of them." caption="Highlights and a comment on one of them." />

## Where to go next

You now have a working library of one. The next things worth ten minutes each:

- [A tour of the app]({base}/docs/tour) — what every part of the window does.
- [Adding papers]({base}/docs/importing) — the other four import paths,
  including BibTeX and the browser extension.
- [Collections and tags]({base}/docs/collections-tags) — organizing once you
  have more than a handful of papers.
- [Search]({base}/docs/search) — one field that covers your library and the web.
- [Keyboard shortcuts]({base}/docs/shortcuts) — the full list.

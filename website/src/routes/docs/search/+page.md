---
layout: docs
title: Search
description: One field that searches your library's full text and OpenAlex, arXiv, and Semantic Scholar at the same time, with sorting and saved searches.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Rotero has one search field, at the top of the library panel, reading "Search
your library and the web...". It does not make you choose between looking
through what you have and looking for what you do not. `⌘L` puts the cursor in
it from anywhere.

## How it behaves

Typing starts a search at three characters, after a 250 ms pause. That pause
means a fast typist sends one query rather than one per keystroke.

Each search runs four lookups at once: your local library's full-text index, and
then OpenAlex, arXiv, and Semantic Scholar. Results are split into two sections:

**In your library** — full-text matches across your own papers, including their
metadata and text.

**From the web** — matches from the three external sources.

The sections stream in as each provider answers, so local results appear
essentially immediately and web results fill in behind them. A slow provider
does not hold up the rest.

<Figure src="search-unified.png" alt="The search field with results below it, split into an In your library section and a From the web section." caption="Local matches appear first; web providers stream in behind them." />

## Importing from the results

Every web result has an **Import** button on its row, and the section header has
**Import All**. Importing pulls the metadata into your library and downloads the
PDF when an open-access copy exists. This is one of the six paths covered in
[Adding papers]({base}/docs/importing).

<Callout type="tip">

Searching a title you half-remember is often faster than opening a browser. The
web section covers arXiv preprints and published records in the same list.

</Callout>

## Sorting

The sort control next to the field applies to the results and to the plain
library list:

- Date Added
- Date Modified
- Title
- Year
- First Author
- Citations

A separate toggle flips between ascending and descending, so "oldest first" and
"newest first" are the same setting in two directions.

## Saved searches

The bookmark icon in the search field saves the current query. Saved searches
appear in their own sidebar section below Tags, and clicking one re-runs it.

This is how you make a filter permanent — a topic you keep returning to, an
author you track, a term you want to check new imports against. Remove a saved
search with the **x** on its row in the sidebar.

<Callout type="note">

A saved search stores the query, not the results it found when you saved it.
Re-running it picks up anything added since.

</Callout>

## Related

- [Collections and tags]({base}/docs/collections-tags) for filtering by
  structure rather than by text.
- [Duplicates]({base}/docs/duplicates) if searching turns up the same paper
  twice.
- `⌘F` opens find, which is a different thing: it searches inside the document
  you are reading. See [The PDF reader]({base}/docs/reader).

---
layout: docs
title: Collections and tags
description: Organize your library with nested collections and colored tags — creating them, filing papers, dragging, renaming, and filtering.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Collections are a folder tree. Tags are colored chips you attach to papers. A
paper can sit in one collection and carry several tags, or sit in none of either
— the fixed Library views still find it.

<Figure src="sidebar-collections.png" alt="The sidebar showing a nested collection tree and a list of colored tag chips below it." caption="Collections and tags occupy their own sidebar sections." />

## Collections

### Creating one

Use the button in the Collections header, press `⌘N`, or use the File menu. The
new collection appears with its name in edit mode, so type and press `Enter`.

To nest one inside another, right-click the parent and choose **New
subcollection**. Nesting has no depth limit.

### The collection menu

Right-click any collection for three items:

- **New subcollection**
- **Rename** — edits the name in place, in the sidebar
- **Delete** — removes the collection; the papers stay in your library

### Filing papers

Three ways, depending on how many papers you are moving:

Drag rows from the paper list onto a collection in the sidebar. This works with
a multi-selection, so `Shift`-click a range first and drag the whole block.

Right-click the selection and use **Add to Collection**, which shows a picker
without leaving the menu.

Or select a single paper and use the Collection picker in the detail panel.

To take a paper out, view the collection, right-click the paper, and choose
**Remove from Collection**. That item only appears while you are inside a
collection, because it needs to know which one to remove from.

### Rearranging the tree

Drag a collection onto another to make it a child. To pull one back out to the
top level, drag it onto the **Move to top level** drop zone that appears while
you are dragging.

<Callout type="note">

Deleting a collection does not delete its papers. They remain in All Papers and
keep their tags. Use the paper context menu's **Delete** if you want the papers
gone.

</Callout>

## Tags

### Colors

Tags are chips in one of six preset colors: Yellow, Red, Green, Blue, Purple,
and Orange. Color is the whole point — it makes a status or a theme readable at
a glance in a long list.

### Creating one

Select a paper and use **+ New tag** in the detail panel, or right-click a
paper, open **Add Tag**, and type into the **New tag...** input at the bottom of
the picker. Either way the tag exists from then on and shows up in the sidebar.

### Assigning

Click a chip in the detail panel's tag picker to add or remove it from the
selected paper.

Or drag papers from the list onto a tag chip in the sidebar. As with
collections, this works on a multi-selection.

### The tag menu

Right-click a tag in the sidebar:

- **Filter by tag** — narrows the paper list to papers carrying it
- **Color swatches** — click one to recolor the tag everywhere at once
- **Rename**
- **Delete**

## Which to use

Collections answer "what project is this for". They are exclusive in practice —
a paper lives in one place in the tree, and the tree mirrors how you think about
your work.

Tags answer everything else, and stack. Reading status, method, dataset,
"cite in intro". Because they are colored and filterable from the sidebar, they
work well for the states a paper passes through.

If you want the filter to persist, run it through the search field and bookmark
it — see [Search]({base}/docs/search) for saved searches.

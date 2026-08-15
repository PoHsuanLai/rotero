---
layout: docs
title: Keyboard shortcuts
description: Every shortcut in Rotero, plus the menu bar and mouse gestures.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

`⌘` means Cmd on macOS and Ctrl everywhere else. Rotero accepts either, so a
shortcut you learned on one platform works on the other.

Every shortcut here except `Esc` can be rebound in
[Settings ▸ Keybindings]({base}/docs/settings#keybindings).

## Global

These work anywhere in the app.

| Shortcut | Command |
| --- | --- |
| `⌘,` | Settings |
| `⌘O` | Open PDF |
| `⌘I` | Import BibTeX |
| `⌘E` | Export BibTeX |
| `⌘F` | Find |
| `⌘L` | Focus library search |
| `⌘W` | Close tab |
| `⌘N` | New collection |
| `⌘1` | Show library |
| `⌘[` | Previous tab |
| `⌘]` | Next tab |
| `⌘Z` | Undo |
| `⌘⇧Z` | Redo |
| `Esc` | Cancel or dismiss |

## In the library

| Shortcut | Command |
| --- | --- |
| `⌘A` | Select all |
| `⌘⇧F` | Toggle favorite |
| `⌘⇧U` | Toggle read |
| `↓` | Select next |
| `↑` | Select previous |
| `Enter` | Open selected |
| `Backspace` / `Delete` | Delete selected |

## What Esc does

`Esc` works down a cascade, handling whatever is most immediate first:

1. Exit annotation mode
2. Close settings
3. Close the PDF find bar
4. Clear the selection

It is the one shortcut you cannot rebind.

<Callout type="note" title="Typing takes priority">

While a text field has focus, Rotero hands `⌘A`, `⌘Z`, `⌘⇧Z`, the arrow keys,
`Enter`, `Backspace`, `⌘⇧F`, and `⌘⇧U` to normal text editing. Select-all in a
search box selects the text, not your papers. Everything else still fires.

</Callout>

## Menu bar

| Menu | Items |
| --- | --- |
| File | Open PDF `⌘O`, Import BibTeX `⌘I`, Export BibTeX `⌘E`, Close Tab `⌘W` |
| Edit | Undo, Redo, Cut, Copy, Paste, Select All, Find `⌘F` |
| View | Library `⌘1`, New Collection `⌘N`, Enter Full Screen |
| Window | Standard window commands |
| Help | Check for Updates… |

## Mouse

| Gesture | What it does |
| --- | --- |
| `⌘`/`Ctrl`-click | Adds a paper to the selection |
| `Shift`-click | Selects a range |
| Right-click | Opens the context menu for what is under the cursor |
| Drag a paper onto a collection or tag | Files or tags it |
| Drag a collection onto another | Reparents it |
| Drag PDFs onto the window | Imports them |
| Drag a panel edge | Resizes the panel |

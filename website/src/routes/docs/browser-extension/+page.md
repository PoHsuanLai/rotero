---
layout: docs
title: Browser extension
description: Install the Rotero Connector for Chrome and save papers from any web page straight into your library.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

The Rotero Connector is a Chrome extension that reads the page you are looking
at, shows you what it found, and saves it to your library in one click. It talks
to Rotero over `127.0.0.1:21984`, so **Rotero has to be running** for the
extension to do anything.

## Install

The extension is not on the Chrome Web Store — listing there needs a paid
developer account — so you load it yourself. Chrome calls this an unpacked
extension and requires Developer mode to allow it.

1. Download `Rotero-Extension-v0.2.2.zip` from the
   [releases page](https://github.com/PoHsuanLai/rotero/releases/latest)
2. Unzip it somewhere you will not delete — Chrome loads the extension from that
   folder every time it starts
3. Open `chrome://extensions/`
4. Turn on **Developer mode** with the toggle in the top right
5. Click **Load unpacked** and select the unzipped folder

<Callout type="note">

The filename carries the version number. The README currently calls it
`Rotero-Extension.zip`; the actual asset on the releases page is
`Rotero-Extension-v0.2.2.zip`.

</Callout>

## Saving a paper

Click the Rotero icon in the toolbar. A 340px panel opens with the extracted
paper and everything you need to file it.

The header reads **Rotero** with a status dot beside it — green when the
connector answers, red when it does not. If Rotero is not running you get a
banner reading "Rotero is not running. Start the app to save papers." Start the
app and reopen the popup.

Below that:

| Part | What it does |
| --- | --- |
| Paper card | The extracted title, authors, journal and year, and DOI |
| Collection tree | Scrollable and nested, rooted at "Library (no collection)" |
| Tag chips | Your tags in their colors, multi-select. Hidden entirely if you have no tags |
| Add button | Reads **Add to Library**, or `Add to "<Collection>"` once you pick one |

Click **Add**. The button changes to **Adding…**, then **Added**, and a result
box appears — green reading "Added to `<Collection>`", or red with the error. If
Rotero quits mid-save you get "Connection lost. Is Rotero running?"

## Which pages it works on

All of them. The popup injects its own extractor into whatever tab is active, so
there is no allowlist of supported publishers. The extension asks for only two
permissions — `activeTab` and `scripting` — and neither grants standing access
to your browsing.

It works best on academic pages, because that is where the metadata it looks for
lives:

- `citation_*`, `DC.*`, and `prism.*` meta tags, which most journals and
  preprint servers emit
- JSON-LD blocks
- arXiv abstract pages, which get their own handling
- Pages that are themselves a PDF

When both the DOI and the author list come up empty, the extension sends the
page's HTML to Rotero and lets the [translators]({base}/docs/importing) have a
go at it instead.

<Callout type="tip">

A page with no usable metadata still saves — you get the title and URL, and can
fill in the rest in Rotero. Running metadata enrichment on the paper afterwards
often fills the gaps on its own.

</Callout>

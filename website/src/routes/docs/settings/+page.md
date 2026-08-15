---
layout: docs
title: Settings
description: Every setting in Rotero, tab by tab — library location, sync, appearance, PDF rendering, keybindings, AI agent, connector, and updates.
---

<script>
  import { base } from '$app/paths';
  import Callout from '$lib/components/docs/Callout.svelte';
  import Figure from '$lib/components/docs/Figure.svelte';
</script>

Open settings with `⌘,`. Six tabs.

## General

<Figure src="settings-general.png" alt="The General settings tab, showing library location, sync, and appearance options." caption="The General tab groups library, sync, and appearance settings." />

| Setting | What it does |
| --- | --- |
| Library location | Folder picker for [where your data lives]({base}/docs/data-locations), with a reset to default. Changing it hot-swaps the database — no restart |
| Sync across devices | Turns [sync]({base}/docs/sync) on |
| Method | Which sync transport to use |
| Sync folder | "Point to a cloud-synced folder (iCloud Drive, Dropbox, etc.)" |
| Dark mode | Toggle |
| UI density | Compact, Default, or Comfortable |
| Auto-fetch metadata on import | On by default. Looks the paper up on CrossRef after you import a PDF |
| Auto-export .bib file | Path picker plus **Clear**. Keeps a `.bib` file in step with your library |

## PDF Viewer

| Setting | Options | Default |
| --- | --- | --- |
| Default zoom | 50, 75, 100, 150, 200, 300% | 150% |
| Selection color | Six swatches | Blue `#339af0` |
| Pages to preload | 3, 5, 10, 20 | 5 |
| Tabs cached in memory | 1–50 | 3 |

**Pages to preload** is how many pages either side of your position Rotero
renders ahead. Higher is smoother to scroll and uses more memory.

**Tabs cached in memory** is how many open PDFs stay rendered. Beyond that
number, tabs are suspended to save memory — they stay open and reopen where you
left them, they just have to re-render.

## Keybindings

<Figure src="settings-keybindings.png" alt="The Keybindings settings tab, listing commands with their shortcut chips." caption="Every rebindable command with its current shortcut." />

Every rebindable command is listed with a chip showing its current shortcut.
Click a chip and it reads "Press a key combination… (Esc to cancel)" — press the
combination you want.

If it collides with an existing binding, Rotero asks whether to reassign, and
warns that the old command loses its shortcut.

Each command has its own reset, and there is a **Reset all**. `Esc` and **Check
for Updates** cannot be rebound.

See [Keyboard shortcuts]({base}/docs/shortcuts) for the defaults.

## AI Agent

Provider selection, an **Account** dropdown of the provider's auth methods, and
an API key field for key-based methods. The
[AI assistant]({base}/docs/ai-assistant) page covers this in full.

## Connector

| Setting | Default | Notes |
| --- | --- | --- |
| Enabled | On | Runs the local HTTP server the [browser extension]({base}/docs/browser-extension) and [Word add-in]({base}/docs/word-addin) use |
| Port | 21984 | 1024–65535. Changes take effect on restart |

<Callout type="note">

Changing the port means the browser extension and the Word add-in stop finding
Rotero — both look for `21984`. Only change it if something else on your machine
already has that port.

</Callout>

## About

| Setting | Default |
| --- | --- |
| Check automatically | On |
| Check Now | Forces an update check |

The version string is shown here too, which is the number to quote in a bug
report.

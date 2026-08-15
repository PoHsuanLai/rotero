---
layout: docs
title: Install
description: Download Rotero for macOS, Windows, or Linux, get past the first-launch security warning, or build it from source.
---

<script>
  import Callout from '$lib/components/docs/Callout.svelte';
</script>

Download the latest build from the
[releases page](https://github.com/PoHsuanLai/rotero/releases/latest) and pick
the file for your platform.

| Platform | File |
| --- | --- |
| macOS (Apple silicon) | `Rotero-*-macos-arm64.dmg` |
| Windows (x64) | `Rotero-*-windows-x64.msi`, or `.zip` to run without installing |
| Linux (x64) | `Rotero-*-linux-x64.deb`, or `.tar.gz` to run without installing |

macOS on Intel is not prebuilt — [build from source](#building-from-source)
instead. iOS and Android are not available yet.

<Callout type="warning" title="Keep the portable archives together">

The `.zip` and `.tar.gz` builds contain the Rotero executable *and* the PDFium
library it loads at runtime. If you move the executable somewhere else on its
own, PDFs will not render. Move the whole folder.

</Callout>

## First launch

Rotero is not signed with a paid developer certificate, so both macOS and
Windows warn about it the first time. This is expected, and you only have to
clear it once.

### macOS

macOS says "Apple could not verify 'Rotero' is free of malware that may harm
your Mac or compromise your privacy."

1. Open **System Settings** and go to **Privacy & Security**
2. Scroll to the message reading "Rotero was blocked to protect your Mac"
3. Click **Open Anyway**

### Windows

SmartScreen shows "Windows protected your PC". Click **More info**, then
**Run anyway**.

### Linux

Install the `.deb` with your package manager, or extract the `.tar.gz` and run
the `rotero` executable inside it. No warning appears.

## Updating

Rotero checks for new versions on its own and offers to install them —
**Help ▸ Check for Updates…** forces a check. The update downloads, replaces the
running application, and asks you to restart.

If your platform has no prebuilt download, the updater says so and links to the
releases page rather than failing silently.

You can turn automatic checks off in **Settings ▸ About**.

## Building from source

You need [Rust](https://rustup.rs/) and [just](https://github.com/casey/just).
PDFium is downloaded for you on the first build.

```sh
git clone https://github.com/PoHsuanLai/rotero.git
cd rotero
just run
```

`just run` builds in debug mode with hot reload, which is slower to start but
rebuilds quickly. For everyday use build a release binary instead:

```sh
just run-release       # build and run
just bundle            # produce a distributable app for your platform
```

Other useful recipes: `just check` (type-check the workspace), `just lint`
(clippy), and `just clean` (remove build artifacts).

<Callout type="note">

The clone needs submodules if you want the full web-import translator corpus.
`git clone --recurse-submodules`, or `git submodule update --init --recursive`
in an existing checkout.

</Callout>

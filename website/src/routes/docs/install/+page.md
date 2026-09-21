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
| Linux (x64) | `Rotero-*-linux-x64.deb` on Debian/Ubuntu, or `.tar.gz` (portable; includes a desktop installer) |

macOS on Intel is not prebuilt — [build from source](#building-from-source)
instead. iOS and Android are not available yet.

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

On Debian/Ubuntu, install the `.deb` with your package manager. On Fedora,
Arch, and other distros, extract the `.tar.gz` and run the installer so Rotero
shows up in the application menu:

```sh
tar -xzf Rotero-*-linux-x64.tar.gz
./install.sh
```

That puts the binary in `~/.local/lib/rotero`, a symlink on `PATH`, a
`.desktop` file, and icons. WebKitGTK 4.1 must be installed (`webkit2gtk4.1`
on Fedora, `libwebkit2gtk-4.1-0` on Debian). No first-launch warning appears.

From a git checkout the same installer is `just install-linux` (downloads the
latest release) or `ROTERO_BIN=./path/to/rotero just install-linux` for a
local build.

## Updating

Rotero checks for new versions on its own and offers to install them —
**Help ▸ Check for Updates…** forces a check. The update downloads, replaces the
running application, and asks you to restart. A Linux desktop install
(`install.sh` / `just install-linux`) is updated in place the same way, including
the launcher icon and `.desktop` file. Development builds (`just run`) and
system packages under `/usr` are left alone — install the portable copy to
receive in-app updates.

If your platform has no prebuilt download, the updater says so and links to the
releases page rather than failing silently.

You can turn automatic checks off in **Settings ▸ About**.

## Building from source

You need [Rust](https://rustup.rs/) and [just](https://github.com/casey/just).
PDF rendering is pure Rust (pdfrum) — no native library download.

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

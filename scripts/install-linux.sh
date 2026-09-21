#!/usr/bin/env bash
#
# Install Rotero as a user-local desktop app on Linux (KDE, GNOME, etc.).
#
# Layout:
#   ~/.local/lib/rotero/rotero     the binary, plus any bundled .so files
#   ~/.local/bin/rotero            symlink onto PATH
#   ~/.local/share/applications/rotero.desktop
#   ~/.local/share/icons/hicolor/*/apps/rotero.png
#
# The in-app updater replaces this same layout: it extracts the release
# tarball, swaps the binary, and refreshes the desktop file and icons. Keep
# the two in sync — see `linux_install_update` in src/updates.rs and
# scripts/stage-linux-tarball.sh.
#
# Usage:
#   scripts/install-linux.sh                  # latest GitHub release
#   scripts/install-linux.sh Rotero-*.tar.gz  # a specific tarball
#   ROTERO_BIN=./path/to/rotero scripts/install-linux.sh   # a local binary
#   ./install.sh                              # from an extracted tarball
#
# Override the destination with ROTERO_LIBDIR / ROTERO_BINDIR / XDG_DATA_HOME.

set -euo pipefail

LIBDIR="${ROTERO_LIBDIR:-$HOME/.local/lib/rotero}"
BINDIR="${ROTERO_BINDIR:-$HOME/.local/bin}"
DATADIR="${XDG_DATA_HOME:-$HOME/.local/share}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# When this file lives in the repo it's scripts/; when shipped in the tarball
# it sits next to the binary as install.sh.
if [ -d "$SCRIPT_DIR/../packaging/linux" ]; then
    REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
else
    REPO_ROOT=""
fi

WORKDIR=""
cleanup() {
    if [ -n "$WORKDIR" ] && [ -d "$WORKDIR" ]; then
        rm -rf "$WORKDIR"
    fi
}
trap cleanup EXIT

die() { echo "error: $*" >&2; exit 1; }

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || die "need '$1' on PATH"
}

# Locate the payload: a directory containing the `rotero` binary and optional
# share/ tree (desktop file + icons).
resolve_payload() {
    if [ -n "${ROTERO_BIN:-}" ]; then
        [ -x "$ROTERO_BIN" ] || die "ROTERO_BIN is not executable: $ROTERO_BIN"
        WORKDIR="$(mktemp -d)"
        cp "$ROTERO_BIN" "$WORKDIR/rotero"
        chmod +x "$WORKDIR/rotero"
        # Sidecars next to a local build (libpdfium.so on older trees).
        local bindir
        bindir="$(cd "$(dirname "$ROTERO_BIN")" && pwd)"
        shopt -s nullglob
        for so in "$bindir"/*.so; do
            cp "$so" "$WORKDIR/"
        done
        shopt -u nullglob
        attach_repo_share "$WORKDIR"
        PAYLOAD="$WORKDIR"
        return
    fi

    if [ -x "$SCRIPT_DIR/rotero" ]; then
        PAYLOAD="$SCRIPT_DIR"
        return
    fi

    if [ "${1:-}" != "" ]; then
        [ -f "$1" ] || die "tarball not found: $1"
        WORKDIR="$(mktemp -d)"
        tar -xzf "$1" -C "$WORKDIR"
        PAYLOAD="$(find_payload_root "$WORKDIR")"
        attach_repo_share "$PAYLOAD"
        return
    fi

    need_cmd curl
    echo "Downloading latest Rotero Linux build…"
    WORKDIR="$(mktemp -d)"
    local json url
    json="$(curl -fsSL -H 'User-Agent: rotero-installer' \
        https://api.github.com/repos/PoHsuanLai/rotero/releases/latest)"
    url="$(printf '%s' "$json" | python3 -c '
import json, sys
rel = json.load(sys.stdin)
assets = rel.get("assets") or []
for a in assets:
    name = a.get("name") or ""
    if name.endswith("linux-x64.tar.gz"):
        print(a["browser_download_url"])
        break
else:
    sys.exit(1)
')" || die "no linux-x64.tar.gz on the latest GitHub release"
    curl -fsSL -o "$WORKDIR/rotero.tar.gz" "$url"
    tar -xzf "$WORKDIR/rotero.tar.gz" -C "$WORKDIR"
    PAYLOAD="$(find_payload_root "$WORKDIR")"
    attach_repo_share "$PAYLOAD"
}

# The tarball is flat (`./rotero`) but tolerate a single wrapping directory.
find_payload_root() {
    local dir="$1"
    if [ -f "$dir/rotero" ]; then
        echo "$dir"
        return
    fi
    local child
    child="$(find "$dir" -mindepth 1 -maxdepth 2 -type f -name rotero | head -1)"
    [ -n "$child" ] || die "no rotero binary in the archive"
    dirname "$child"
}

# Older release tarballs shipped only the binary. Fill in the desktop file and
# icons from this checkout so `just install-linux` against v0.2.6 still produces
# a launcher entry.
attach_repo_share() {
    local dest="$1"
    if [ -d "$dest/share/applications" ]; then
        return
    fi
    [ -n "$REPO_ROOT" ] || return
    [ -f "$REPO_ROOT/packaging/linux/rotero.desktop" ] || return
    mkdir -p "$dest/share/applications"
    cp "$REPO_ROOT/packaging/linux/rotero.desktop" "$dest/share/applications/"
    if [ -d "$REPO_ROOT/packaging/linux/share/icons" ]; then
        mkdir -p "$dest/share/icons"
        cp -a "$REPO_ROOT/packaging/linux/share/icons/." "$dest/share/icons/"
    fi
    if [ -f "$REPO_ROOT/assets/icon.png" ]; then
        mkdir -p "$dest/share/icons/hicolor/1024x1024/apps"
        cp "$REPO_ROOT/assets/icon.png" "$dest/share/icons/hicolor/1024x1024/apps/rotero.png"
    fi
}

install_payload() {
    local payload="$1"
    local bin="$payload/rotero"
    [ -f "$bin" ] || die "no rotero binary in $payload"
    chmod +x "$bin"

    mkdir -p "$LIBDIR" "$BINDIR" "$DATADIR/applications"

    install -m 755 "$bin" "$LIBDIR/rotero"

    shopt -s nullglob
    for so in "$payload"/*.so; do
        install -m 644 "$so" "$LIBDIR/"
    done
    shopt -u nullglob

    ln -sfn "$LIBDIR/rotero" "$BINDIR/rotero"

    local desktop_src="$payload/share/applications/rotero.desktop"
    if [ -f "$desktop_src" ]; then
        sed -e "s|^Exec=.*|Exec=$LIBDIR/rotero|" \
            -e "s|^TryExec=.*|TryExec=$LIBDIR/rotero|" \
            "$desktop_src" > "$DATADIR/applications/rotero.desktop"
        chmod 644 "$DATADIR/applications/rotero.desktop"
    fi

    if [ -d "$payload/share/icons" ]; then
        mkdir -p "$DATADIR/icons"
        cp -a "$payload/share/icons/." "$DATADIR/icons/"
    fi

    update-desktop-database "$DATADIR/applications" >/dev/null 2>&1 || true
    if [ -d "$DATADIR/icons/hicolor" ]; then
        gtk-update-icon-cache -f -t "$DATADIR/icons/hicolor" >/dev/null 2>&1 || true
    fi
    kbuildsycoca6 --noincremental >/dev/null 2>&1 || \
        kbuildsycoca5 --noincremental >/dev/null 2>&1 || true
}

resolve_payload "${1:-}"
install_payload "$PAYLOAD"

echo "Installed Rotero to $LIBDIR"
echo "Launcher: $DATADIR/applications/rotero.desktop"
echo "Open it from the application menu, or run: $BINDIR/rotero"

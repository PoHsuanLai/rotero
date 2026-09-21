#!/usr/bin/env bash
#
# Build the portable Linux tarball the in-app updater downloads.
#
# Layout (must stay in lockstep with linux_install_update in src/updates.rs
# and with scripts/install-linux.sh):
#
#   rotero
#   *.so                         optional, next to the binary ($ORIGIN)
#   share/applications/rotero.desktop
#   share/icons/hicolor/*/apps/rotero.png
#   install.sh                   first-time installer
#
# Usage: scripts/stage-linux-tarball.sh [output.tar.gz]
# Finds the dx-built binary under target/dx/. Override with ROTERO_BIN.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TAG="${GITHUB_REF_NAME:-$(git -C "$ROOT" describe --tags --abbrev=0 2>/dev/null || echo dev)}"
OUT="${1:-$ROOT/Rotero-${TAG}-linux-x64.tar.gz}"

die() { echo "error: $*" >&2; exit 1; }

if [ -n "${ROTERO_BIN:-}" ]; then
    BIN="$ROTERO_BIN"
else
    BIN="$(find "$ROOT/target/dx/rotero/release" -type f -name rotero -perm -u+x 2>/dev/null | head -1 || true)"
fi
[ -n "$BIN" ] && [ -f "$BIN" ] || die "rotero binary not found (build with dx bundle --release, or set ROTERO_BIN)"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp "$BIN" "$STAGE/rotero"
chmod +x "$STAGE/rotero"

BINDIR="$(cd "$(dirname "$BIN")" && pwd)"
shopt -s nullglob
for so in "$BINDIR"/*.so; do
    cp "$so" "$STAGE/"
done
shopt -u nullglob

mkdir -p "$STAGE/share/applications"
cp "$ROOT/packaging/linux/rotero.desktop" "$STAGE/share/applications/"

if [ -d "$ROOT/packaging/linux/share/icons" ]; then
    mkdir -p "$STAGE/share/icons"
    cp -a "$ROOT/packaging/linux/share/icons/." "$STAGE/share/icons/"
fi
if [ -f "$ROOT/assets/icon.png" ]; then
    mkdir -p "$STAGE/share/icons/hicolor/1024x1024/apps"
    cp "$ROOT/assets/icon.png" "$STAGE/share/icons/hicolor/1024x1024/apps/rotero.png"
fi

cp "$ROOT/scripts/install-linux.sh" "$STAGE/install.sh"
chmod +x "$STAGE/install.sh"

mkdir -p "$(dirname "$OUT")"
tar -czf "$OUT" -C "$STAGE" .
echo "$OUT"

#!/usr/bin/env bash
# Captures the documentation screenshots from a seeded fixture library.
#
#     website/tooling/capture.sh [shot-id ...]
#
# With no arguments every shot in shots.toml is captured; otherwise only the
# ids given. macOS only — it drives `screencapture` against a real window,
# which is also why these images are committed rather than built in CI.
#
# The app runs against a throwaway library via ROTERO_DATA_DIR, so a capture
# run never reads or writes the real one.

set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
OUT="$ROOT/website/static/docs/shots"
# The Settings shot shows the library path, and a /var/folders/j5/kbh6k9hd…
# temp path reads as a bug — so the library lives under ~/Rotero, which looks
# like something a person would choose.
FIXTURE="${ROTERO_SHOT_FIXTURE:-$HOME/Rotero (screenshots)}"
# The .app bundle stays in $TMPDIR: launched from under ~/Library/Application
# Support, macOS starts the process but never gives it a window.
BUNDLE_DIR="${TMPDIR:-/tmp}/rotero-shot-bundle"
SCRIPT_FILE="$FIXTURE/.shot-script"
APP_LOG="$FIXTURE/.app.log"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "capture.sh needs macOS (screencapture); shots are committed to the repo." >&2
  exit 1
fi

command -v just >/dev/null || { echo "just is required" >&2; exit 1; }

APP_PID=""
cleanup() {
  [[ -n "$APP_PID" ]] && kill "$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
}
trap cleanup EXIT

echo "==> Seeding fixture library at $FIXTURE"
cargo run -q -p rotero-db --example seed_fixture -- "$FIXTURE"

mkdir -p "$OUT"
: >"$SCRIPT_FILE"

echo "==> Building the app"
# Debug build: the shot driver is compiled out of release builds by design.
cargo build -q -p rotero

# macOS only gives a process a window if it looks like an app bundle; a bare
# executable launched from a shell stays an accessory process with no window.
# This wraps the freshly built binary in the minimum bundle that satisfies that,
# rather than depending on `dx bundle` (which builds release, without the driver).
APP_BUNDLE="$BUNDLE_DIR/Rotero.app"
rm -rf "$APP_BUNDLE"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
cp "$ROOT/target/debug/rotero" "$APP_BUNDLE/Contents/MacOS/Rotero"
cp "$ROOT/lib/libpdfium.dylib" "$APP_BUNDLE/Contents/MacOS/" 2>/dev/null || true
cat >"$APP_BUNDLE/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Rotero</string>
  <key>CFBundleDisplayName</key><string>Rotero</string>
  <key>CFBundleExecutable</key><string>Rotero</string>
  <key>CFBundleIdentifier</key><string>com.rotero.Rotero.shots</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.0.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

echo "==> Launching against the fixture"
# Bigger than the app's 1200x800 default: at that size the sidebar labels and
# the detail panel crop, which reads as a cramped app in a screenshot. Sized to
# sit inside a 1920x1080 logical screen with room for the menu bar.
PDFIUM_DYNAMIC_LIB_PATH="$ROOT/lib" \
ROTERO_DATA_DIR="$FIXTURE" \
ROTERO_SHOT_SCRIPT="$SCRIPT_FILE" \
ROTERO_WINDOW_SIZE="${ROTERO_SHOT_WINDOW:-1760x1040}" \
  "$APP_BUNDLE/Contents/MacOS/Rotero" >"$APP_LOG" 2>&1 &
APP_PID=$!

echo "==> Waiting for the window"
WINDOW_ID=""
for attempt in $(seq 1 60); do
  if WINDOW_ID="$(swift "$HERE/window-id.swift" Rotero 2>/dev/null)"; then
    [[ -n "$WINDOW_ID" ]] && break
  fi
  # An unfocused app can sit outside the on-screen window list. Nudging it to
  # the front makes it register; without this the wait sometimes times out even
  # though the process started fine.
  if [[ $((attempt % 5)) -eq 0 ]]; then
    osascript -e 'tell application "System Events" to set frontmost of every process whose unix id is '"$APP_PID"' to true' >/dev/null 2>&1 || true
  fi
  sleep 1
done
if [[ -z "$WINDOW_ID" ]]; then
  echo "" >&2
  echo "Could not find the Rotero window." >&2
  if [[ "$(swift "$HERE/list-windows.swift" 2>/dev/null | awk -F'\t' '{print $2}' | sort -u | wc -l)" -le 1 ]]; then
    cat >&2 <<'HINT'

Only this terminal's own windows are visible to the window list, which means
macOS has not granted it Screen Recording permission. Without it,
CGWindowListCopyWindowInfo hides other applications' windows and
screencapture cannot target them.

Grant it in System Settings > Privacy & Security > Screen Recording, add the
terminal you are running this from, then restart that terminal and re-run.
HINT
  else
    echo "The app may have failed to start; see $APP_LOG" >&2
  fi
  exit 1
fi
echo "    window $WINDOW_ID"

# Let the first library load and render settle before the first shot.
sleep 3

# Sends the steps for one shot and blocks until the driver reports back.
run_steps() {
  local steps="$1"
  printf '%s\n' "$steps" >"$SCRIPT_FILE"
  for _ in $(seq 1 120); do
    [[ "$(cat "$SCRIPT_FILE" 2>/dev/null || true)" == "done" ]] && return 0
    sleep 0.25
  done
  echo "    warning: steps did not complete in time" >&2
  return 0
}

# shots.toml is read by a small python helper rather than parsed in bash,
# because the step tables do not survive line-based parsing.
# `mapfile` is bash 4+; macOS ships bash 3.2, so read the ids the portable way.
SHOT_IDS=()
while IFS= read -r line; do
  [[ -n "$line" ]] && SHOT_IDS+=("$line")
done < <(python3 "$HERE/shots.py" ids "$HERE/shots.toml" "$@")

for id in "${SHOT_IDS[@]}"; do
  file="$(python3 "$HERE/shots.py" file "$HERE/shots.toml" "$id")"
  steps="$(python3 "$HERE/shots.py" steps "$HERE/shots.toml" "$id")"

  echo "==> $id"
  # Return to a known state so shots do not depend on each other. Reloading
  # does not work — this is a single-page WebView, so a reload leaves the
  # dialog open. Escape cascades through the app's own dismiss chain instead.
  run_steps "$(printf '%s\n' \
    'eval document.querySelector(".settings-overlay .detail-close")?.click()' \
    'wait 250' \
    'eval document.querySelector(".modal-overlay .detail-close")?.click()' \
    'wait 250' \
    'eval [...document.querySelectorAll(".pdf-tab-close")].forEach(b => b.click())' \
    'wait 500' \
    'eval (() => { const s = document.querySelector("#library-search-input"); if (s && s.value) { s.value = ""; s.dispatchEvent(new Event("input", { bubbles: true })); } })()' \
    'wait 500' \
    'eval document.querySelector(".sidebar-nav-item")?.click()' \
    'wait 800')"

  [[ -n "$steps" ]] && run_steps "$steps"
  sleep 1

  screencapture -l"$WINDOW_ID" -o -x "$OUT/$file"
  echo "    $OUT/$file"
done

echo "==> Done. $(( ${#SHOT_IDS[@]} )) shot(s) written to $OUT"

# The marketing hero and the README use two of these shots. Copying them here
# keeps them from drifting back to a hand-taken screenshot of someone's real
# library — which is what they were before this script existed.
if [[ -f "$OUT/library-overview.png" && -f "$OUT/pdf-annotations.png" ]]; then
  cp "$OUT/library-overview.png" "$ROOT/website/static/screenshot-library.png"
  cp "$OUT/pdf-annotations.png" "$ROOT/website/static/screenshot-reader.png"
  cp "$OUT/library-overview.png" "$ROOT/assets/screenshot-library.png"
  cp "$OUT/pdf-annotations.png" "$ROOT/assets/screenshot-reader.png"
  echo "==> Updated the hero and README screenshots"
fi

# A step that silently fails to land captures whatever was already on screen,
# which shows up as two identical files. Cheap to detect, and otherwise only
# noticed by eye long after the fact.
DUPES="$(md5 -q "$OUT"/*.png 2>/dev/null | sort | uniq -d)"
if [[ -n "$DUPES" ]]; then
  echo "" >&2
  echo "warning: some shots are byte-identical, so their steps probably did not apply:" >&2
  while IFS= read -r hash; do
    for f in "$OUT"/*.png; do
      [[ "$(md5 -q "$f")" == "$hash" ]] && echo "  - $(basename "$f")" >&2
    done
  done <<<"$DUPES"
fi

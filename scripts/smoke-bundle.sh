#!/usr/bin/env bash
#
# Launch a built Rotero bundle against a throwaway library and assert it works.
#
# This exists because `cargo test` passes against code the shipped binary does
# not run: the app had its own database-open path, that path skipped CRR
# initialization, and every write committed its row and then failed change
# tracking. Tags and notes vanished on reload and nothing synced, while CI stayed
# green.
#
# So the assertions here deliberately run *outside* the process, against the real
# artifact:
#   - the app launches and stays up
#   - the library it created passes verify_database_health
#   - the browser connector is listening
#   - a paper saved over the connector API lands in the library
#   - a tag attached to that paper survives a restart   <- the money assertion
#
# No UI automation: the connector's HTTP API is a shipped control surface, it
# needs no window, and it runs on hosted CI. The screenshot driver cannot be used
# because it is compiled out of release builds — i.e. absent from exactly the
# artifact users install.
#
# Usage: scripts/smoke-bundle.sh <path-to-bundle-or-binary>

set -euo pipefail

APP="${1:-}"
if [ -z "$APP" ]; then
    echo "usage: $0 <path-to-.app-or-binary>" >&2
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The connector port lives in the config, which ROTERO_DATA_DIR redirects along
# with the library — so a throwaway data dir gets the default port.
PORT="${ROTERO_SMOKE_PORT:-21984}"
WORKDIR="$(mktemp -d)"
DATA_DIR="$WORKDIR/library"
# The app logs to $HOME/rotero-debug.log, not stdout. Redirecting HOME keeps this
# run's log isolated and out of the developer's real one.
FAKE_HOME="$WORKDIR/home"
LOG="$FAKE_HOME/rotero-debug.log"
STDOUT_LOG="$WORKDIR/stdout.log"
APP_PID=""

pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1" >&2; FAILED=1; }
FAILED=0

# The app's own log plus anything it wrote to stdout/stderr.
dump_log() {
    [ -f "$LOG" ] && tail -30 "$LOG" >&2
    [ -s "$STDOUT_LOG" ] && tail -10 "$STDOUT_LOG" >&2
    true
}

# Search the app log. A missing log is a failure, not a pass: the checks below
# assert that certain errors are *absent*, so treating "no file" as "no error"
# scored a crash before logging started as a success.
require_log() {
    if [ ! -f "$LOG" ]; then
        fail "no log at $LOG — the app did not get far enough to write one"
    fi
}

log_has() {
    grep -qi "$1" "$LOG"
}

# Resolve the executable inside a macOS .app, or accept a bare binary.
resolve_binary() {
    if [ -d "$APP" ] && [ -d "$APP/Contents/MacOS" ]; then
        find "$APP/Contents/MacOS" -maxdepth 1 -type f -perm -111 | head -1
    else
        echo "$APP"
    fi
}

BIN="$(resolve_binary)"
if [ ! -x "$BIN" ]; then
    echo "no executable found at $APP" >&2
    exit 2
fi

cleanup() {
    if [ -n "$APP_PID" ] && kill -0 "$APP_PID" 2>/dev/null; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

# Launch against an isolated library so a developer's real one is never touched.
#
# PDFIUM_DYNAMIC_LIB_PATH is deliberately NOT set: a shipped bundle does not get
# it, so setting it here would hide the exact resolution failure this is meant to
# catch.
launch() {
    HOME="$FAKE_HOME" ROTERO_DATA_DIR="$DATA_DIR" "$BIN" >>"$STDOUT_LOG" 2>&1 &
    APP_PID=$!
}

stop() {
    if [ -n "$APP_PID" ] && kill -0 "$APP_PID" 2>/dev/null; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
    APP_PID=""
}

# The connector binding is the readiness signal: it starts after the database is
# open, so polling it avoids racing startup with a fixed sleep.
#
# Polls `/api/token` rather than `/api/status`: the API now requires a token, so
# an unauthenticated status probe answers 401 whether or not the app is ready.
# The pairing endpoint answers as soon as the server is listening, which is the
# signal wanted here.
wait_for_connector() {
    local deadline=$((SECONDS + 45))
    while [ $SECONDS -lt $deadline ]; do
        if curl -sf "http://127.0.0.1:$PORT/api/token" >/dev/null 2>&1; then
            return 0
        fi
        if [ -n "$APP_PID" ] && ! kill -0 "$APP_PID" 2>/dev/null; then
            echo "app exited during startup; log:" >&2
            dump_log
            return 1
        fi
        sleep 1
    done
    return 1
}

echo "smoke: $BIN"
echo "       library $DATA_DIR"
mkdir -p "$DATA_DIR" "$FAKE_HOME"

# --- 0. seed a tag to attach later -------------------------------------------
# The central assertion below is that a tag survives a restart, and a tag can
# only be attached by id — the connector API has no way to create one. Seeding
# here rather than skipping means the assertion always runs; a skipped check
# that reports nothing is how this class of bug stayed hidden.
if ! cargo run -q --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p rotero-db --example create_tag -- "$DATA_DIR" "smoke-tag" >/dev/null 2>&1; then
    fail "could not seed a tag into the throwaway library"
    exit 1
fi
pass "seeded a tag into the library"

# --- 1. launches and stays up -------------------------------------------------
launch
if wait_for_connector; then
    pass "app launched and the connector is listening on $PORT"
else
    fail "app did not become ready within 45s"
    dump_log
    exit 1
fi

if kill -0 "$APP_PID" 2>/dev/null; then
    pass "process still alive after startup"
else
    fail "process exited after startup"
fi

# --- 2. save a paper, with a tag, over the connector -------------------------
# The API requires the shared token; /api/token hands it to a non-web caller.
TOKEN="$(curl -sf "http://127.0.0.1:$PORT/api/token" |
    sed -n 's/.*"token"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')"
if [ -z "$TOKEN" ]; then
    fail "could not obtain a connector token"
    dump_log
    exit 1
fi
pass "paired with the connector"

# A tag is what the shipped bug lost: add_tag_to_paper committed its junction
# row and then failed change tracking, so the tag was gone on the next launch.
#
# The tag has to exist before it can be attached — `/api/save` takes `tag_ids`
# and nothing in the API creates one — and a throwaway library starts empty. It
# is read back from the running app rather than trusted from the seeding step,
# so a tag the app cannot see fails here instead of later.
TAG_ID="$(curl -sf "http://127.0.0.1:$PORT/api/tags" -H "X-Rotero-Token: $TOKEN" |
    sed -n 's/.*"id"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
if [ -z "$TAG_ID" ]; then
    fail "the seeded tag was not visible over the API"
    dump_log
    exit 1
fi
pass "seeded tag is visible to the app"

SAVE_BODY="{\"title\":\"Smoke Test Paper\",\"authors\":[\"Ada Lovelace\"],\"year\":1843,\"tag_ids\":[\"$TAG_ID\"]}"

if curl -sf -X POST "http://127.0.0.1:$PORT/api/save" \
    -H 'Content-Type: application/json' \
    -H "X-Rotero-Token: $TOKEN" \
    -d "$SAVE_BODY" >/dev/null; then
    pass "POST /api/save accepted"
else
    fail "POST /api/save failed"
fi

# An unauthenticated request must not work: that is the whole point of the
# token, and a regression here reopens the library to any page the user visits.
if curl -sf "http://127.0.0.1:$PORT/api/tags" >/dev/null 2>&1; then
    fail "an untokened request was served — the API is open to any local caller"
else
    pass "untokened requests are rejected"
fi

# The connector returns before the insert completes, so give it a moment.
sleep 3
stop

# --- 3. the library the app produced is structurally sound --------------------
# `attach_readonly` inside the checker means this reports what the app left on
# disk; opening normally would run crr.init() and repair the very defect this is
# looking for.
if cargo run -q --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p rotero-db --example verify_health -- "$DATA_DIR" >/dev/null 2>&1; then
    pass "library passes verify_database_health"
else
    fail "library is structurally unhealthy"
    cargo run -q --manifest-path "$REPO_ROOT/Cargo.toml" \
        -p rotero-db --example verify_health -- "$DATA_DIR" || true
fi

# --- 4. the saved paper persisted across a restart ---------------------------
# Asserted against the database rather than an HTTP response: this has to be
# true of what is on disk after the process is gone, which is exactly what the
# shipped bug got wrong (the row committed, the call returned Err, and the write
# was discarded by callers).
PAPER_COUNT="$(cargo run -q --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p rotero-db --example count_papers -- "$DATA_DIR" 2>/dev/null || echo 0)"
if [ "${PAPER_COUNT:-0}" -ge 1 ]; then
    pass "saved paper persisted after shutdown (papers=$PAPER_COUNT)"
else
    fail "saved paper did not persist (papers=${PAPER_COUNT:-0})"
fi

# --- 4b. the tag survived too — the assertion this script exists for ---------
# add_tag_to_paper wrote its junction row and then failed change tracking, so
# the row was committed while the caller saw an error and discarded it. Locally
# the tag looked attached until the next launch. Only a read from disk after the
# process is gone can tell the difference.
TAGGED="$(cargo run -q --manifest-path "$REPO_ROOT/Cargo.toml" \
    -p rotero-db --example count_tagged_papers -- "$DATA_DIR" 2>/dev/null || echo 0)"
if [ "${TAGGED:-0}" -ge 1 ]; then
    pass "tag survived the restart (tagged papers=$TAGGED)"
else
    fail "the tag did not persist — junction rows are not being written"
fi

# --- 5. the app reopens the library it created --------------------------------
# A library that opens once but not twice is a migration or CRR-state bug, and
# it is the shape a user hits on second launch rather than first.
launch
if wait_for_connector; then
    pass "app reopened the existing library"
else
    fail "app failed to reopen the library it created"
    dump_log
fi
stop

# --- 6. PDF engine bound ------------------------------------------------------
# The next two checks assert an error is *absent*, so the log has to exist for
# either to mean anything.
require_log

# Absence of a failure line is the signal: a bind error is recorded and logged.
if log_has "Failed to bind PDFium"; then
    fail "PDFium failed to bind (PDF viewing, annotations, and thumbnails are dead)"
    grep -i "pdfium" "$LOG" | head -3 >&2
else
    pass "PDF engine bound"
fi

# --- 7. no database health failure was recorded at startup --------------------
if log_has "Database health check failed"; then
    fail "startup preflight reported an unhealthy database"
    grep -i "Database health check failed" "$LOG" | head -3 >&2
else
    pass "startup preflight reported a healthy database"
fi

echo
if [ "$FAILED" -eq 0 ]; then
    echo "smoke: PASS"
else
    echo "smoke: FAIL" >&2
    echo "log at $LOG (preserved on failure):" >&2
    cp "$LOG" "${TMPDIR:-/tmp}/rotero-smoke-failure.log" 2>/dev/null || true
    echo "  copied to ${TMPDIR:-/tmp}/rotero-smoke-failure.log" >&2
fi
exit "$FAILED"

# Rotero development tasks

# Default: list available recipes
default:
    @just --list

# Build the project (debug)
build:
    dx build

# Build the project (release)
build-release:
    dx build --release

# Run the app (debug, with hot-reload)
run:
    dx serve

# Run the app (release)
run-release:
    dx serve --release

# Bundle the desktop app for distribution
bundle:
    dx bundle --release

# Install Rotero as a user-local desktop app (Linux).
# Pass a tarball, or omit to download the latest GitHub release.
# For a just-built binary: ROTERO_BIN=./path/to/rotero just install-linux
install-linux TARBALL="":
    {{justfile_directory()}}/scripts/install-linux.sh {{TARBALL}}

# Run the test suite.
# Needs no network: provider tests run against a local stub.
test: setup-nextest
    cargo nextest run --workspace

# Run the sync property tests with far more, and longer, generated scenarios.
#
# The same tests `just test` runs, with a bigger budget rather than a separate
# `#[ignore]`d copy: an ignored test compiles but never runs, so it rots without
# anyone noticing. Worth running before touching the merge or the clock.
proptest-deep: setup-nextest
    ROTERO_PROPTEST=heavy \
        cargo nextest run -p rotero-db -E 'binary(sync_props)' --no-fail-fast

# Install cargo-nextest if it is not already present.
#
# nextest runs each test in its own process with real parallelism, which takes
# the suite from ~90s to ~13s. Installed from a prebuilt binary rather than
# compiled from source — building it takes longer than the time it saves.
#
# It does not run doctests, which is its one gap versus `cargo test`. The
# workspace has none; if that changes, add a `cargo test --doc` step alongside.
#
# cargo-binstall itself is installed the same way if missing, so a fresh
# checkout needs nothing beyond a Rust toolchain.
setup-nextest:
    #!/usr/bin/env bash
    set -euo pipefail

    if command -v cargo-nextest >/dev/null 2>&1; then
        exit 0
    fi

    if ! command -v cargo-binstall >/dev/null 2>&1; then
        echo "Installing cargo-binstall..."
        curl -L --proto '=https' --tlsv1.2 -sSf \
            https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh \
            | bash
    fi

    echo "Installing cargo-nextest..."
    cargo binstall --no-confirm cargo-nextest

# Launch a built app and assert it works: database health, connector, and a
# saved paper that persists. Pass a .app bundle or a binary.
smoke BUNDLE="target/dx/rotero/release/macos/Rotero.app":
    {{justfile_directory()}}/scripts/smoke-bundle.sh {{BUNDLE}}

# Check all crates compile
check:
    cargo check --workspace

# Run clippy on all crates, exactly as CI does
#
# `--all-targets` and `-D warnings` both matter: without them this passes on
# code CI rejects, because tests are a separate target and a warning is only
# fatal in CI. Keep this identical to the Clippy step in .github/workflows.
lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Clean build artifacts
clean:
    cargo clean

# Clean everything
clean-all: clean

# Test the browser connector API (app must be running)
test-connector:
    curl -s http://127.0.0.1:21984/api/status | python3 -m json.tool

# Send a test paper to the connector (app must be running)
test-save-paper:
    curl -s -X POST http://127.0.0.1:21984/api/save \
        -H "Content-Type: application/json" \
        -d '{"title":"Test Paper","doi":"10.1234/test","authors":["Test Author"]}' \
        | python3 -m json.tool

# Serve iOS app on simulator
run-ios device="iPhone 17 Pro":
    xcrun simctl boot "{{device}}" 2>/dev/null || true
    dx serve --platform ios --features mobile --no-default-features

# Bundle iOS app for device
build-ios:
    dx bundle --platform ios --features "mobile" --no-default-features

# Capture the user guide screenshots (macOS only; pass shot ids to redo a subset)
docs-screenshots *SHOTS:
    {{justfile_directory()}}/website/tooling/capture.sh {{SHOTS}}

# Capture the extension popup and Word task pane (headless; pass popup/taskpane
# to redo one). Separate from docs-screenshots because these need no GUI.
docs-screenshots-web *SHOTS:
    cd {{justfile_directory()}}/website && node tooling/capture-web.mjs {{SHOTS}}

# Report how much of the app the user guide documents
docs-coverage:
    node {{justfile_directory()}}/website/tooling/coverage/check.mjs

# Serve the website (including the guide) with hot reload
docs-dev:
    cd {{justfile_directory()}}/website && npm run dev

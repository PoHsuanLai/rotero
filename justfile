# Rotero development tasks

# Default: list available recipes
default:
    @just --list

# Download PDFium binary for the current platform
setup-pdfium:
    #!/usr/bin/env bash
    set -euo pipefail

    PDFIUM_DIR="{{justfile_directory()}}/lib"
    mkdir -p "$PDFIUM_DIR"

    ARCH=$(uname -m)
    OS=$(uname -s)

    # Where the shared library sits inside the archive. Windows ships the DLL
    # under bin/ (lib/ holds only the import library); the others use lib/.
    ARCHIVE_SUBDIR="lib"

    if [ "$OS" = "Darwin" ]; then
        if [ "$ARCH" = "arm64" ]; then
            PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-mac-arm64.tgz"
        else
            PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-mac-x64.tgz"
        fi
        LIB_NAME="libpdfium.dylib"
    elif [ "$OS" = "Linux" ]; then
        if [ "$ARCH" = "aarch64" ]; then
            PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-linux-arm64.tgz"
        else
            PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-linux-x64.tgz"
        fi
        LIB_NAME="libpdfium.so"
    else
        # MSYS/MinGW/Cygwin shells on Windows report MINGW64_NT-*, CYGWIN_NT-*, etc.
        case "$OS" in
            MINGW*|MSYS*|CYGWIN*)
                if [ "$ARCH" = "aarch64" ]; then
                    PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-arm64.tgz"
                else
                    PDFIUM_URL="https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-x64.tgz"
                fi
                LIB_NAME="pdfium.dll"
                ARCHIVE_SUBDIR="bin"
                ;;
            *)
                echo "Unsupported OS: $OS"
                exit 1
                ;;
        esac
    fi

    if [ -f "$PDFIUM_DIR/$LIB_NAME" ]; then
        echo "PDFium already downloaded at $PDFIUM_DIR/$LIB_NAME"
        exit 0
    fi

    echo "Downloading PDFium for $OS $ARCH..."
    TMP=$(mktemp -d)
    curl -sL "$PDFIUM_URL" -o "$TMP/pdfium.tgz"
    tar -xzf "$TMP/pdfium.tgz" -C "$TMP"

    cp "$TMP/$ARCHIVE_SUBDIR/$LIB_NAME" "$PDFIUM_DIR/$LIB_NAME"
    rm -rf "$TMP"

    echo "PDFium installed to $PDFIUM_DIR/$LIB_NAME"

# Build the project (debug)
build: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" dx build

# Build the project (release)
build-release: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" dx build --release

# Run the app (debug, with hot-reload)
run: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" dx serve

# Run the app (release)
run-release: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" dx serve --release

# Bundle the desktop app for distribution
bundle: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" dx bundle --release

# Run the test suite (PDFium is downloaded first so the PDF tests don't skip).
# Needs no network: provider tests run against a local stub.
test: setup-pdfium setup-nextest
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" cargo nextest run --workspace

# Run the sync property tests with far more, and longer, generated scenarios.
#
# The same tests `just test` runs, with a bigger budget rather than a separate
# `#[ignore]`d copy: an ignored test compiles but never runs, so it rots without
# anyone noticing. Worth running before touching the merge or the clock.
proptest-deep: setup-pdfium setup-nextest
    ROTERO_PROPTEST=heavy \
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" \
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

# Launch a built app and assert it works: database health, connector, a saved
# paper that persists, and PDFium resolution. Pass a .app bundle or a binary.
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

# Clean PDFium binary
clean-pdfium:
    rm -rf {{justfile_directory()}}/lib

# Clean everything
clean-all: clean clean-pdfium

# Test the browser connector API (app must be running)
test-connector:
    curl -s http://127.0.0.1:21984/api/status | python3 -m json.tool

# Send a test paper to the connector (app must be running)
test-save-paper:
    curl -s -X POST http://127.0.0.1:21984/api/save \
        -H "Content-Type: application/json" \
        -d '{"title":"Test Paper","doi":"10.1234/test","authors":["Test Author"]}' \
        | python3 -m json.tool

# Download static PDFium for iOS (from paulocoutinhox/pdfium-lib)
setup-pdfium-ios:
    #!/usr/bin/env bash
    set -euo pipefail

    DEVICE_DIR="{{justfile_directory()}}/lib/ios-device"
    SIM_DIR="{{justfile_directory()}}/lib/ios-sim"

    if [ -f "$DEVICE_DIR/libpdfium.a" ] && [ -f "$SIM_DIR/libpdfium.a" ]; then
        echo "PDFium iOS static libs already present"
        exit 0
    fi

    echo "Downloading static PDFium for iOS from paulocoutinhox/pdfium-lib..."
    TMP=$(mktemp -d)
    gh release download --repo paulocoutinhox/pdfium-lib --pattern "ios.tgz" --dir "$TMP"
    mkdir -p "$TMP/extracted"
    tar -xzf "$TMP/ios.tgz" -C "$TMP/extracted"

    mkdir -p "$DEVICE_DIR" "$SIM_DIR"
    cp "$TMP/extracted/release/lib/device/libpdfium.a" "$DEVICE_DIR/libpdfium.a"
    cp "$TMP/extracted/release/lib/simulator/libpdfium.a" "$SIM_DIR/libpdfium.a"
    rm -rf "$TMP"

    # Thin fat archives to single-arch (rustc requires thin archives)
    lipo "$DEVICE_DIR/libpdfium.a" -thin arm64 -output "$DEVICE_DIR/libpdfium-thin.a" && mv "$DEVICE_DIR/libpdfium-thin.a" "$DEVICE_DIR/libpdfium.a"

    # Also download dynamic lib for simulator (from bblanchon — works around libc++ ABI mismatch)
    if [ ! -f "$SIM_DIR/libpdfium.dylib" ]; then
        TMP2=$(mktemp -d)
        gh release download --repo bblanchon/pdfium-binaries --pattern "pdfium-ios-simulator-arm64.tgz" --dir "$TMP2"
        tar -xzf "$TMP2/pdfium-ios-simulator-arm64.tgz" -C "$TMP2"
        cp "$TMP2/lib/libpdfium.dylib" "$SIM_DIR/libpdfium.dylib"
        rm -rf "$TMP2"
    fi

    echo "PDFium iOS libs installed to lib/ios-device/ and lib/ios-sim/"

# Serve iOS app on simulator (dynamic PDFium linking — sim supports dylibs)
run-ios device="iPhone 17 Pro": setup-pdfium-ios
    xcrun simctl boot "{{device}}" 2>/dev/null || true
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib/ios-sim" \
    dx serve --platform ios --features mobile --no-default-features

# Bundle iOS app for device (static PDFium linking — required for real devices)
build-ios: setup-pdfium-ios
    PDFIUM_STATIC_LIB_PATH="{{justfile_directory()}}/lib/ios-device" \
    dx bundle --platform ios --features "mobile,pdfium-static" --no-default-features

# Capture the user guide screenshots (macOS only; pass shot ids to redo a subset)
docs-screenshots *SHOTS: setup-pdfium
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

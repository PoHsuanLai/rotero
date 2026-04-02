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
        echo "Unsupported OS: $OS"
        exit 1
    fi

    if [ -f "$PDFIUM_DIR/$LIB_NAME" ]; then
        echo "PDFium already downloaded at $PDFIUM_DIR/$LIB_NAME"
        exit 0
    fi

    echo "Downloading PDFium for $OS $ARCH..."
    TMP=$(mktemp -d)
    curl -sL "$PDFIUM_URL" -o "$TMP/pdfium.tgz"
    tar -xzf "$TMP/pdfium.tgz" -C "$TMP"

    cp "$TMP/lib/$LIB_NAME" "$PDFIUM_DIR/$LIB_NAME"
    rm -rf "$TMP"

    echo "PDFium installed to $PDFIUM_DIR/$LIB_NAME"

# Build the project (debug)
build: setup-pdfium
    cargo build

# Build the project (release)
build-release: setup-pdfium
    cargo build --release

# Run the app (debug)
run: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" cargo run

# Run the app (release)
run-release: setup-pdfium
    PDFIUM_DYNAMIC_LIB_PATH="{{justfile_directory()}}/lib" cargo run --release

# Check all crates compile
check:
    cargo check --workspace

# Run clippy on all crates
lint:
    cargo clippy --workspace -- -W clippy::all

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

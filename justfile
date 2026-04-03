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
    dx serve --platform ios --features mobile --no-default-features --device "{{device}}"

# Bundle iOS app for device (static PDFium linking — required for real devices)
build-ios: setup-pdfium-ios
    PDFIUM_STATIC_LIB_PATH="{{justfile_directory()}}/lib/ios-device" \
    dx bundle --platform ios --features "mobile,pdfium-static" --no-default-features

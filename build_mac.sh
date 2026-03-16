#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Nebula DeEsser — macOS Build Script
# Builds universal CLAP plugin (Apple Silicon + Intel x86_64)
# Optimized for Core Audio
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

PLUGIN_NAME="nebula_desser"

echo "╔════════════════════════════════════════════════╗"
echo "║  NEBULA DEESSER — macOS Universal Build        ║"
echo "╚════════════════════════════════════════════════╝"
echo ""

if ! command -v cargo &>/dev/null; then
    echo "[ERROR] cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi

# Add both targets for universal binary
echo "[*] Adding Rust targets for universal binary..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Install bundler
if ! cargo nih-plug --help &>/dev/null 2>&1; then
    echo "[*] Installing cargo-nih-plug..."
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
fi

echo "[*] Building for aarch64 (Apple Silicon)..."
RUSTFLAGS="-C target-cpu=apple-m1 -C opt-level=3 -C lto=fat -C codegen-units=1" \
    cargo build --release --target aarch64-apple-darwin

echo "[*] Building for x86_64 (Intel)..."
RUSTFLAGS="-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1" \
    cargo build --release --target x86_64-apple-darwin

echo "[*] Creating universal binary with lipo..."
PLUGIN_LIB="lib${PLUGIN_NAME}.dylib"
AARCH64_LIB="target/aarch64-apple-darwin/release/${PLUGIN_LIB}"
X86_64_LIB="target/x86_64-apple-darwin/release/${PLUGIN_LIB}"
UNIVERSAL_LIB="target/universal/${PLUGIN_LIB}"

mkdir -p target/universal
lipo -create "$AARCH64_LIB" "$X86_64_LIB" -output "$UNIVERSAL_LIB"
echo "[✓] Universal binary created: $UNIVERSAL_LIB"

# Bundle as CLAP
echo "[*] Bundling CLAP plugin..."

CLAP_BUNDLE="target/bundled/Nebula DeEsser.clap"
mkdir -p "$CLAP_BUNDLE/Contents/MacOS"

# Copy binary
cp "$UNIVERSAL_LIB" "$CLAP_BUNDLE/Contents/MacOS/nebula_desser"

# Generate Info.plist
cat > "$CLAP_BUNDLE/Contents/Info.plist" << 'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>nebula_desser</string>
    <key>CFBundleIdentifier</key>
    <string>audio.nebula.deesser</string>
    <key>CFBundleName</key>
    <string>Nebula DeEsser</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>NSPrincipalClass</key>
    <string>NSObject</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
</dict>
</plist>
PLIST

echo ""
echo "[✓] Build complete!"
echo "[✓] CLAP: $CLAP_BUNDLE"
echo ""
echo "Install to ~/Library/Audio/Plug-Ins/CLAP/ with:"
echo "  cp -r \"$CLAP_BUNDLE\" ~/Library/Audio/Plug-Ins/CLAP/"
echo ""
echo "Core Audio optimization: use a buffer size of 128 or 256 in Logic Pro"
echo "for best performance. Nebula DeEsser supports M1/M2/M3 native ARM64."

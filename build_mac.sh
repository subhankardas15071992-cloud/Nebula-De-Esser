#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Nebula DeEsser v2.0.0 — macOS Build Script
# Builds Universal CLAP plugin (Apple Silicon ARM64 + Intel x86_64)
# Optimized for Core Audio
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

PLUGIN_NAME="nebula_desser"
PLUGIN_DISPLAY="Nebula DeEsser"
PLUGIN_VERSION="2.0.0"
BUNDLE_ID="audio.nebula.deesser"

echo "╔════════════════════════════════════════════════╗"
echo "║  NEBULA DEESSER v2.0.0 — macOS Universal Build ║"
echo "╚════════════════════════════════════════════════╝"
echo ""

if ! command -v cargo &>/dev/null; then
    echo "[ERROR] cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi

if ! command -v lipo &>/dev/null; then
    echo "[ERROR] lipo not found. Install Xcode Command Line Tools:"
    echo "  xcode-select --install"
    exit 1
fi

# Add both targets
echo "[*] Adding Rust targets for universal binary..."
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Install bundler
if ! cargo nih-plug --help &>/dev/null 2>&1; then
    echo "[*] Installing cargo-nih-plug..."
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
fi

echo "[*] Building for aarch64-apple-darwin (Apple Silicon)..."
RUSTFLAGS="-C target-cpu=apple-m1 -C opt-level=3 -C lto=fat -C codegen-units=1" \
    cargo build --release --target aarch64-apple-darwin

echo "[*] Building for x86_64-apple-darwin (Intel 64-bit)..."
RUSTFLAGS="-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1" \
    cargo build --release --target x86_64-apple-darwin

echo "[*] Creating universal binary with lipo..."
PLUGIN_LIB="lib${PLUGIN_NAME}.dylib"
AARCH64_LIB="target/aarch64-apple-darwin/release/${PLUGIN_LIB}"
X86_64_LIB="target/x86_64-apple-darwin/release/${PLUGIN_LIB}"
UNIVERSAL_LIB="target/universal/${PLUGIN_LIB}"

mkdir -p target/universal
lipo -create "$AARCH64_LIB" "$X86_64_LIB" -output "$UNIVERSAL_LIB"
echo "[✓] Universal binary: $UNIVERSAL_LIB"
lipo -info "$UNIVERSAL_LIB"

# Bundle as CLAP
echo "[*] Bundling CLAP plugin..."
CLAP_BUNDLE="target/bundled/${PLUGIN_DISPLAY}.clap"
mkdir -p "${CLAP_BUNDLE}/Contents/MacOS"

cp "$UNIVERSAL_LIB" "${CLAP_BUNDLE}/Contents/MacOS/${PLUGIN_NAME}"

cat > "${CLAP_BUNDLE}/Contents/Info.plist" << PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${PLUGIN_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleName</key>
    <string>${PLUGIN_DISPLAY}</string>
    <key>CFBundleVersion</key>
    <string>${PLUGIN_VERSION}</string>
    <key>CFBundleShortVersionString</key>
    <string>${PLUGIN_VERSION}</string>
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
echo "[✓] Build complete! — ${PLUGIN_DISPLAY} v${PLUGIN_VERSION}"
echo "[✓] CLAP bundle: ${CLAP_BUNDLE}"
echo ""
echo "Install with:"
echo "  mkdir -p ~/Library/Audio/Plug-Ins/CLAP"
echo "  cp -r \"${CLAP_BUNDLE}\" ~/Library/Audio/Plug-Ins/CLAP/"
echo ""
echo "─── Core Audio tips ─────────────────────────────────────────────────────"
echo "Buffer size 128–256 recommended in Logic/Ableton for best performance."
echo "Native ARM64 support for M1/M2/M3/M4 — no Rosetta needed."
echo ""
echo "─── New in v2.0.0 ───────────────────────────────────────────────────────"
echo "  • Presets: save/load envelope settings"
echo "  • Undo / Redo (50-step history)"
echo "  • MIDI Learn for all main parameters"
echo "  • FX Bypass (soft bypass)"
echo "  • Input / Output Level + Pan knobs"
echo "  • Oversampling: Off / 2x / 4x / 6x / 8x"
echo "  • Fixed spectrum analyzer (live FFT display)"

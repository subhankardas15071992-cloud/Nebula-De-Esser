#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Nebula DeEsser — Linux Build Script
# Builds CLAP plugin optimized for x86_64 (JACK / ALSA / PipeWire)
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

PLUGIN_NAME="nebula_desser"
OUT_DIR="target/bundled"

echo "╔════════════════════════════════════════════════╗"
echo "║  NEBULA DEESSER — Linux Build                  ║"
echo "╚════════════════════════════════════════════════╝"
echo ""

# Ensure Rust toolchain is available
if ! command -v cargo &>/dev/null; then
    echo "[ERROR] cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi

# Install cargo-nih-plug bundler if not present
if ! cargo nih-plug --help &>/dev/null 2>&1; then
    echo "[*] Installing cargo-nih-plug..."
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
fi

echo "[*] Building release binary (LTO, codegen-units=1)..."
RUSTFLAGS="-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1" \
    cargo build --release 2>&1

echo "[*] Bundling CLAP plugin..."
cargo nih-plug bundle "${PLUGIN_NAME}" --release 2>&1

echo ""
echo "[✓] Build complete!"
echo ""

# Find and display output files
CLAP_FILE=$(find target -name "*.clap" 2>/dev/null | head -1)
if [ -n "$CLAP_FILE" ]; then
    echo "[✓] CLAP: $CLAP_FILE"
    echo ""
    echo "Install to ~/.clap/ with:"
    echo "  mkdir -p ~/.clap && cp \"$CLAP_FILE\" ~/.clap/"
else
    echo "[!] CLAP file not found in target/. Check build output above."
fi

echo ""
echo "JACK optimization: use jackd -d alsa -r 44100 -p 128 for lowest latency."
echo "For PipeWire: configure /etc/pipewire/pipewire.conf for 64-sample periods."

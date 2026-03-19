#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────────
# Nebula DeEsser v2.0.0 — Linux Build Script
# Builds 64-bit CLAP plugin (x86_64) for JACK / ALSA / PipeWire
# ─────────────────────────────────────────────────────────────────────────────
set -euo pipefail

PLUGIN_NAME="nebula_desser"
PLUGIN_VERSION="2.0.0"

echo "╔════════════════════════════════════════════════╗"
echo "║  NEBULA DEESSER v2.0.0 — Linux Build           ║"
echo "╚════════════════════════════════════════════════╝"
echo ""

# Ensure Rust toolchain is available
if ! command -v cargo &>/dev/null; then
    echo "[ERROR] cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi

# Ensure we're building for 64-bit explicitly
if [[ "$(uname -m)" != "x86_64" ]]; then
    echo "[WARN] Not running on x86_64. Attempting cross-compile is not supported by this script."
    echo "       Please run on a 64-bit x86 Linux machine."
    exit 1
fi

# Install cargo-nih-plug bundler if not present
if ! cargo nih-plug --help &>/dev/null 2>&1; then
    echo "[*] Installing cargo-nih-plug..."
    cargo install --git https://github.com/robbert-vdh/nih-plug.git cargo-nih-plug
fi

echo "[*] Building release binary (64-bit, LTO, codegen-units=1)..."
RUSTFLAGS="-C target-cpu=x86-64-v2 -C opt-level=3 -C lto=fat -C codegen-units=1" \
    cargo build --release 2>&1

echo "[*] Bundling CLAP plugin..."
cargo nih-plug bundle "${PLUGIN_NAME}" --release 2>&1

echo ""
echo "[✓] Build complete! — Nebula DeEsser v${PLUGIN_VERSION}"
echo ""

CLAP_FILE=$(find target -name "*.clap" 2>/dev/null | head -1)
if [ -n "$CLAP_FILE" ]; then
    echo "[✓] CLAP: $CLAP_FILE"
    SIZE=$(du -sh "$CLAP_FILE" | cut -f1)
    echo "[✓] Size: $SIZE"
    echo ""
    echo "Install to ~/.clap/ with:"
    echo "  mkdir -p ~/.clap && cp \"$CLAP_FILE\" ~/.clap/"
    echo ""
    echo "Or system-wide:"
    echo "  sudo mkdir -p /usr/lib/clap && sudo cp \"$CLAP_FILE\" /usr/lib/clap/"
else
    echo "[!] CLAP file not found in target/. Check build output above."
    exit 1
fi

echo ""
echo "─── Audio backend tips ───────────────────────────────────────────────────"
echo "JACK: jackd -d alsa -r 44100 -p 128 --softmode"
echo "PipeWire: set quantum=128 in /etc/pipewire/pipewire.conf for lowest latency"
echo "ALSA: Use hw: device with dmix disabled for direct access"
echo ""
echo "─── New in v2.0.0 ────────────────────────────────────────────────────────"
echo "  • Presets: save/load envelope settings"
echo "  • Undo / Redo (50-step history)"
echo "  • MIDI Learn for all main parameters"
echo "  • FX Bypass (soft bypass)"
echo "  • Input / Output Level + Pan knobs"
echo "  • Oversampling: Off / 2x / 4x / 6x / 8x"
echo "  • Fixed spectrum analyzer (live FFT display)"

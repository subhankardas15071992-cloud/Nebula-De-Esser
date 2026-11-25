#!/usr/bin/env bash
# Nebula De-Esser v3.4 - macOS per-architecture bundle build.
# Builds separate VST3 bundles for Apple Silicon and Intel macOS.

set -euo pipefail

PLUGIN_NAME="nebula_desser"
PLUGIN_DISPLAY="Nebula De-Esser"
PLUGIN_VERSION="3.4.0"
PLUGIN_VERSION_DISPLAY="3.4"

if [[ "$#" -gt 0 ]]; then
    TARGETS=("$@")
else
    TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")
fi

echo "================================================"
echo "  NEBULA DE-ESSER v${PLUGIN_VERSION_DISPLAY} - macOS Builds"
echo "================================================"
echo ""

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "[ERROR] This script is for macOS only."
    exit 1
fi

if ! xcode-select -p &>/dev/null; then
    echo "[ERROR] Xcode Command Line Tools not found."
    echo "Install with: xcode-select --install"
    exit 1
fi

if ! command -v rustc &>/dev/null; then
    echo "[ERROR] Rust compiler not found."
    echo "Install from: https://www.rust-lang.org/tools/install"
    exit 1
fi

HOST_TARGET="$(rustc -vV | awk '/^host: / { print $2 }')"
export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-10.13}"

echo "[OK] macOS $(sw_vers -productVersion) on $(uname -m)"
echo "[OK] Xcode: $(xcodebuild -version 2>/dev/null | head -n1 || echo 'Command Line Tools only')"
echo "[OK] Rust: $(rustc --version | cut -d' ' -f2)"
echo "[OK] MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET}"
echo ""

echo "[*] Installing requested Rust targets..."
rustup target add "${TARGETS[@]}"

for target in "${TARGETS[@]}"; do
    case "$target" in
        aarch64-apple-darwin)
            label="Apple Silicon"
            ;;
        x86_64-apple-darwin)
            label="Intel"
            ;;
        *)
            label="$target"
            ;;
    esac

    echo ""
    echo "------------------------------------------------"
    echo "  Building ${PLUGIN_DISPLAY} v${PLUGIN_VERSION} for ${label}"
    echo "------------------------------------------------"

    # Remove stale bundles from earlier runs so only the current format and
    # architecture can be picked up below.
    rm -rf target/bundled

    cargo xtask bundle "$PLUGIN_NAME" --release --target "$target"

    # nih_plug_xtask always writes to target/bundled, even for cross builds.
    # Snapshot each architecture so the next target cannot overwrite it.
    local_bundle_dir="target/$target/bundled"
    rm -rf "$local_bundle_dir"
    mkdir -p "$local_bundle_dir"
    cp -R "target/bundled/${PLUGIN_DISPLAY}.vst3" "$local_bundle_dir/"

    if [[ "${NEBULA_SKIP_TESTS:-0}" != "1" && "$target" == "$HOST_TARGET" ]]; then
        echo "[*] Running tests for native target ${target}..."
        cargo test --release --target "$target" -- --test-threads=1
    else
        echo "[*] Skipping test execution for target ${target}."
    fi

    echo "[OK] ${label} bundles:"
    echo "     target/${target}/bundled/${PLUGIN_DISPLAY}.vst3"
done

echo ""
echo "================================================"
echo "Build complete. Separate macOS bundles were written under:"
for target in "${TARGETS[@]}"; do
    echo "  target/${target}/bundled/"
done
echo "================================================"

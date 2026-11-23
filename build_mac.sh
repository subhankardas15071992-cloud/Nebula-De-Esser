#!/usr/bin/env bash
# Nebula De-Esser v3.4 - macOS per-architecture bundle build.
# Builds separate CLAP + VST3 + AUv2 (and AUv3 when full Xcode is available)
# bundles for Apple Silicon and Intel macOS.
#
# Environment:
#   NEBULA_BUILD_AU=0         build only CLAP + VST3
#   NEBULA_BUILD_AUV3=auto    build AUv3 only when full Xcode is available (default)
#   NEBULA_BUILD_AUV3=0       never build AUv3
#   NEBULA_BUILD_AUV3=1       require AUv3 (fails if full Xcode is missing)
#   CMAKE_BIN=/path/to/cmake  use a specific CMake binary

set -euo pipefail

PLUGIN_NAME="nebula_desser"
PLUGIN_DISPLAY="Nebula De-Esser"
PLUGIN_VERSION="3.4.0"
PLUGIN_VERSION_DISPLAY="3.4"
AU_CMAKE_DIR="$PWD/cmake/macos-au"

if [[ "$#" -gt 0 ]]; then
    TARGETS=("$@")
else
    TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")
fi

BUILD_AU="${NEBULA_BUILD_AU:-1}"
BUILD_AUV3="${NEBULA_BUILD_AUV3:-auto}"

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

HAS_FULL_XCODE=0
if xcodebuild -version &>/dev/null && xcrun --find ibtool &>/dev/null; then
    HAS_FULL_XCODE=1
fi

if [[ "$BUILD_AU" == "1" ]]; then
    CMAKE_BIN="${CMAKE_BIN:-cmake}"
    if ! command -v "$CMAKE_BIN" &>/dev/null; then
        echo "[ERROR] CMake is required for AUv2/AUv3 builds."
        echo "Install CMake from https://cmake.org/download/ or set CMAKE_BIN."
        echo "To build only CLAP + VST3: NEBULA_BUILD_AU=0 ./build_mac.sh"
        exit 1
    fi
    if [[ ! -f "$AU_CMAKE_DIR/CMakeLists.txt" ]]; then
        echo "[ERROR] AU CMake project not found at $AU_CMAKE_DIR"
        exit 1
    fi
fi

if [[ "$BUILD_AU" == "1" && "$BUILD_AUV3" == "1" && "$HAS_FULL_XCODE" != "1" ]]; then
    echo "[ERROR] AUv3 requires a full Xcode installation, not just the Command Line Tools."
    echo "Install Xcode from the App Store, then run: sudo xcode-select -s /Applications/Xcode.app"
    echo "Use NEBULA_BUILD_AUV3=auto (default) or NEBULA_BUILD_AU=0 to skip AUv3."
    exit 1
fi

echo "[OK] macOS $(sw_vers -productVersion) on $(uname -m)"
echo "[OK] Xcode: $(xcodebuild -version 2>/dev/null | head -n1 || echo 'Command Line Tools only')"
echo "[OK] Rust: $(rustc --version | cut -d' ' -f2)"
echo "[OK] MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET}"
if [[ "$BUILD_AU" == "1" ]]; then
    echo "[OK] CMake: $("$CMAKE_BIN" --version | head -n1)"
fi
if [[ "$BUILD_AU" == "1" && "$HAS_FULL_XCODE" != "1" ]]; then
    if [[ "$BUILD_AUV3" == "0" ]]; then
        echo "[i] AUv3 disabled; building AUv2 only."
    else
        echo "[i] AUv3 will be skipped: full Xcode is not installed. AUv2 will still be built."
    fi
fi
echo ""

echo "[*] Installing requested Rust targets..."
rustup target add "${TARGETS[@]}"

cmake_arch_for_target() {
    case "$1" in
        aarch64-apple-darwin)
            echo "arm64"
            ;;
        x86_64-apple-darwin)
            echo "x86_64"
            ;;
        *)
            echo "[ERROR] Unsupported macOS target: $1" >&2
            exit 1
            ;;
    esac
}

build_auv2() {
    local target="$1"
    local cmake_arch="$2"
    local clap_bundle="$PWD/target/$target/bundled/${PLUGIN_DISPLAY}.clap"
    local build_dir="$PWD/target/$target/au-build/auv2"

    if [[ ! -d "$clap_bundle" ]]; then
        echo "[ERROR] CLAP bundle missing at $clap_bundle"
        exit 1
    fi

    echo "[*] Building AUv2 wrapper (${cmake_arch})..."
    rm -rf "$build_dir"
    "$CMAKE_BIN" -S "$AU_CMAKE_DIR" -B "$build_dir" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_OSX_ARCHITECTURES="$cmake_arch" \
        -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 \
        -DNEBULA_VERSION="$PLUGIN_VERSION" \
        -DNEBULA_CLAP_BUNDLE="$clap_bundle" \
        -DCLAP_SDK_ROOT="$PWD/vendor/clap-sdk" \
        -DAUDIOUNIT_SDK_ROOT="$PWD/vendor/AudioUnitSDK"
    "$CMAKE_BIN" --build "$build_dir" --target nebula_auv2 --parallel "${AU_BUILD_JOBS:-$(sysctl -n hw.ncpu)}"

    local src="$build_dir/artifacts/${PLUGIN_DISPLAY}.component"
    if [[ ! -d "$src" ]]; then
        src="$(find "$build_dir" -maxdepth 5 -type d -name "${PLUGIN_DISPLAY}.component" | head -n1)"
    fi
    if [[ -z "${src:-}" || ! -d "$src" ]]; then
        echo "[ERROR] AUv2 bundle was not produced."
        exit 1
    fi

    rm -rf "target/$target/bundled/${PLUGIN_DISPLAY}.component"
    cp -R "$src" "target/$target/bundled/"
}

build_auv3() {
    local target="$1"
    local cmake_arch="$2"
    local clap_bundle="$PWD/target/$target/bundled/${PLUGIN_DISPLAY}.clap"
    local build_dir="$PWD/target/$target/au-build/auv3"

    echo "[*] Building AUv3 appex and host app (${cmake_arch})..."
    rm -rf "$build_dir"
    "$CMAKE_BIN" -G Xcode -S "$AU_CMAKE_DIR" -B "$build_dir" \
        -DCMAKE_OSX_ARCHITECTURES="$cmake_arch" \
        -DCMAKE_OSX_DEPLOYMENT_TARGET=11.0 \
        -DCMAKE_XCODE_ATTRIBUTE_CODE_SIGN_IDENTITY=- \
        -DNEBULA_BUILD_AUV3=ON \
        -DNEBULA_VERSION="$PLUGIN_VERSION" \
        -DNEBULA_CLAP_BUNDLE="$clap_bundle" \
        -DCLAP_SDK_ROOT="$PWD/vendor/clap-sdk" \
        -DAUDIOUNIT_SDK_ROOT="$PWD/vendor/AudioUnitSDK"
    "$CMAKE_BIN" --build "$build_dir" --config Release --target nebula_auv3_host

    local src="$build_dir/artifacts/Release/${PLUGIN_DISPLAY} AUv3.app"
    if [[ ! -d "$src" ]]; then
        src="$(find "$build_dir" -maxdepth 6 -type d -name "${PLUGIN_DISPLAY} AUv3.app" | head -n1)"
    fi
    if [[ -z "${src:-}" || ! -d "$src" ]]; then
        echo "[ERROR] AUv3 host app was not produced."
        exit 1
    fi

    rm -rf "target/$target/bundled/${PLUGIN_DISPLAY} AUv3.app"
    cp -R "$src" "target/$target/bundled/"
}

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
    cmake_arch="$(cmake_arch_for_target "$target")"

    echo ""
    echo "------------------------------------------------"
    echo "  Building ${PLUGIN_DISPLAY} v${PLUGIN_VERSION} for ${label}"
    echo "------------------------------------------------"

    cargo xtask bundle "$PLUGIN_NAME" --release --target "$target"

    # nih_plug_xtask always writes to target/bundled, even for cross builds.
    # Snapshot each architecture so the next target cannot overwrite it.
    local_bundle_dir="target/$target/bundled"
    rm -rf "$local_bundle_dir"
    mkdir -p "$local_bundle_dir"
    cp -R "target/bundled/${PLUGIN_DISPLAY}.clap" "$local_bundle_dir/"
    cp -R "target/bundled/${PLUGIN_DISPLAY}.vst3" "$local_bundle_dir/"

    if [[ "$target" == "$HOST_TARGET" ]]; then
        echo "[*] Running tests for native target ${target}..."
        cargo test --release --target "$target" -- --test-threads=1
    else
        echo "[*] Skipping test execution for non-native target ${target}."
    fi

    if [[ "$BUILD_AU" == "1" ]]; then
        build_auv2 "$target" "$cmake_arch"

        if [[ "$HAS_FULL_XCODE" == "1" && "$BUILD_AUV3" != "0" ]]; then
            build_auv3 "$target" "$cmake_arch"
        fi
    fi

    echo "[OK] ${label} bundles:"
    echo "     target/${target}/bundled/${PLUGIN_DISPLAY}.clap"
    echo "     target/${target}/bundled/${PLUGIN_DISPLAY}.vst3"
    if [[ "$BUILD_AU" == "1" ]]; then
        echo "     target/${target}/bundled/${PLUGIN_DISPLAY}.component"
        if [[ -d "target/$target/bundled/${PLUGIN_DISPLAY} AUv3.app" ]]; then
            echo "     target/${target}/bundled/${PLUGIN_DISPLAY} AUv3.app"
        fi
    fi
done

echo ""
echo "================================================"
echo "Build complete. Separate macOS bundles were written under:"
for target in "${TARGETS[@]}"; do
    echo "  target/${target}/bundled/"
done
if [[ "$BUILD_AU" == "1" ]]; then
    echo ""
    echo "AUv2 install:"
    echo "  cp -R \"target/<arch>/bundled/${PLUGIN_DISPLAY}.component\" ~/Library/Audio/Plug-Ins/Components/"
    echo ""
    echo "AUv3 install (run the app once so macOS registers the extension):"
    echo "  cp -R \"target/<arch>/bundled/${PLUGIN_DISPLAY} AUv3.app\" /Applications/"
    echo "  open \"/Applications/${PLUGIN_DISPLAY} AUv3.app\""
fi
echo "================================================"

#!/usr/bin/env bash
# make_universal.sh — Merges the x86_64 and aarch64 nih-plug bundles produced
# by `cargo xtask bundle --target <arch>` into a single macOS Universal Binary
# using `lipo`.
#
# Expected layout before this script runs:
#   target/x86_64-apple-darwin/bundled/<Plugin>.clap/
#   target/x86_64-apple-darwin/bundled/<Plugin>.vst3/
#   target/aarch64-apple-darwin/bundled/<Plugin>.clap/
#   target/aarch64-apple-darwin/bundled/<Plugin>.vst3/
#
# Produces:
#   target/universal/bundled/<Plugin>.clap/   ← Universal Binary inside
#   target/universal/bundled/<Plugin>.vst3/   ← Universal Binary inside

set -euo pipefail

ARM_DIR="target/aarch64-apple-darwin/bundled"
X86_DIR="target/x86_64-apple-darwin/bundled"
UNI_DIR="target/universal/bundled"

# ── Sanity checks ─────────────────────────────────────────────────────────────
if [[ ! -d "$ARM_DIR" ]]; then
    echo "ERROR: aarch64 bundle directory not found: $ARM_DIR" >&2
    exit 1
fi
if [[ ! -d "$X86_DIR" ]]; then
    echo "ERROR: x86_64 bundle directory not found: $X86_DIR" >&2
    exit 1
fi

mkdir -p "$UNI_DIR"

# ── Process every bundle (.clap, .vst3, or any future format) ─────────────────
for arm_bundle in "$ARM_DIR"/*/; do
    [[ -d "$arm_bundle" ]] || continue

    bundle_name="$(basename "$arm_bundle")"
    x86_bundle="$X86_DIR/$bundle_name"
    uni_bundle="$UNI_DIR/$bundle_name"

    if [[ ! -d "$x86_bundle" ]]; then
        echo "WARNING: No x86_64 counterpart for '$bundle_name' — skipping." >&2
        continue
    fi

    echo "──────────────────────────────────────────────"
    echo "Universal: $bundle_name"

    # Start with the arm64 bundle as the structural template
    # (preserves Info.plist, PkgInfo, Resources, …)
    rm -rf "$uni_bundle"
    cp -R "$arm_bundle" "$uni_bundle"

    # nih-plug places the Mach-O binary in Contents/MacOS/<binary>
    macos_dir="$uni_bundle/Contents/MacOS"
    if [[ ! -d "$macos_dir" ]]; then
        echo "  WARNING: Contents/MacOS not found in '$bundle_name', skipping." >&2
        continue
    fi

    for uni_binary in "$macos_dir"/*; do
        [[ -f "$uni_binary" ]] || continue
        binary_name="$(basename "$uni_binary")"

        arm_binary="$arm_bundle/Contents/MacOS/$binary_name"
        x86_binary="$x86_bundle/Contents/MacOS/$binary_name"

        if [[ ! -f "$arm_binary" ]]; then
            echo "  SKIP (no arm64 binary): $binary_name" >&2
            continue
        fi
        if [[ ! -f "$x86_binary" ]]; then
            echo "  SKIP (no x86_64 binary): $binary_name" >&2
            continue
        fi

        echo "  lipo → $binary_name"
        lipo -create "$x86_binary" "$arm_binary" -output "$uni_binary"

        # Confirm the result
        echo "    $(lipo -archs "$uni_binary")"
    done

    echo "  ✓ $uni_bundle"
done

echo ""
echo "══════════════════════════════════════════════"
echo "Universal bundles written to: $UNI_DIR"
ls -1 "$UNI_DIR"
echo "══════════════════════════════════════════════"

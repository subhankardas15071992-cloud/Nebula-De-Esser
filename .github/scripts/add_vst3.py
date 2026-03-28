#!/usr/bin/env python3
"""
add_vst3.py — Patches src/lib.rs to add VST3 export support alongside the
existing CLAP export that Nebula De-Esser ships with.

What it does
────────────
1. Reads src/lib.rs from the workspace root.
2. Detects the plugin struct name by scanning for `impl ClapPlugin for <Name>`
   (preferred) or `impl Plugin for <Name>` (fallback).
3. Appends an `impl Vst3Plugin for <Name>` block and the
   `nih_export_vst3!(<Name>)` macro call.

Idempotent — calling it a second time is a no-op (detected via the presence
of `nih_export_vst3!` in the file).

Licensing note
──────────────
nih-plug's VST3 bindings link against the Steinberg VST3 SDK, which is
licensed under GPLv3. If your plugin uses a permissive licence (MIT, ISC,
Apache-2.0 …) the VST3 build artefact will be GPLv3 by virtue of linking.
The CLAP build is unaffected. See https://github.com/robbert-vdh/nih-plug#licensing.
"""

import re
import sys
from pathlib import Path


# ── helpers ──────────────────────────────────────────────────────────────────

def make_class_id(plugin_name: str) -> str:
    """
    Derive a stable 16-byte ASCII VST3 class ID from the plugin struct name.

    The ID is padded/truncated to exactly 16 characters so it can be used
    directly with the `*b"..."` byte-string syntax in Rust.

    IMPORTANT: Once you have shipped a VST3 plugin, this value must never
    change — hosts use it to identify saved states.  Commit the patched
    lib.rs (or hard-code the ID in the source) before your first public
    release if you want persistence across rebuilds.
    """
    seed = plugin_name + "VST3_NebulaDE"   # deterministic suffix
    padded = (seed + "X" * 16)[:16]        # always exactly 16 ASCII chars
    return padded


# ── main ──────────────────────────────────────────────────────────────────────

def main() -> None:
    lib_rs = Path("src/lib.rs")

    if not lib_rs.exists():
        print(
            "ERROR: src/lib.rs not found.\n"
            "       Run this script from the Cargo workspace root.",
            file=sys.stderr,
        )
        sys.exit(1)

    content = lib_rs.read_text(encoding="utf-8")

    # ── Idempotency guard ────────────────────────────────────────────────────
    if "nih_export_vst3!" in content:
        print("✓ VST3 export already present in src/lib.rs — nothing to patch.")
        sys.exit(0)

    # ── Detect the plugin struct name ────────────────────────────────────────
    # Prefer `impl ClapPlugin for X` because every nih-plug CLAP plugin has it.
    match = re.search(r"\bimpl\s+ClapPlugin\s+for\s+(\w+)", content)
    if not match:
        match = re.search(r"\bimpl\s+Plugin\s+for\s+(\w+)", content)
    if not match:
        print(
            "ERROR: Cannot locate the plugin struct in src/lib.rs.\n"
            "       Expected to find one of:\n"
            "         impl ClapPlugin for <StructName> { … }\n"
            "         impl Plugin     for <StructName> { … }",
            file=sys.stderr,
        )
        sys.exit(1)

    plugin_name = match.group(1)
    class_id    = make_class_id(plugin_name)

    print(f"  Plugin struct  : {plugin_name}")
    print(f"  VST3_CLASS_ID  : *b\"{class_id}\"  ({len(class_id)} bytes)")

    # ── Build the VST3 impl block ─────────────────────────────────────────────
    vst3_block = f"""\
// ─── VST3 export (added by .github/scripts/add_vst3.py) ──────────────────────
//
// Vst3Plugin is a thin wrapper trait over Plugin.  The two mandatory constants
// are:
//   VST3_CLASS_ID   — 16-byte unique plugin identifier (must never change after
//                     your first public release).
//   VST3_SUBCATEGORIES — displayed in the host's plugin browser.
//
// All other Vst3Plugin constants have sensible defaults; override them in
// src/lib.rs after the initial scaffolding if you need to customise bus names,
// MIDI support flags, etc.

impl Vst3Plugin for {plugin_name} {{
    // DO NOT change this value once you have shipped the plugin publicly.
    const VST3_CLASS_ID: [u8; 16] = *b"{class_id}";

    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Dynamics,
    ];
}}

nih_export_vst3!({plugin_name});
"""

    # Append after the last non-blank line, then a single trailing newline.
    patched = content.rstrip("\n") + "\n\n" + vst3_block
    lib_rs.write_text(patched, encoding="utf-8")

    print(f"✓ src/lib.rs patched — VST3 support added for '{plugin_name}'.")


if __name__ == "__main__":
    main()

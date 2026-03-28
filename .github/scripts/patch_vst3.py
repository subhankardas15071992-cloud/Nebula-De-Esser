#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
patch_vst3.py
=============
Appends the two declarations nih-plug needs to build a VST3 bundle alongside
the existing CLAP bundle:

    impl Vst3Plugin for <PluginStruct> { ... }
    nih_export_vst3!(<PluginStruct>);

The struct name is detected automatically by scanning src/lib.rs for the
`impl ClapPlugin for X` block that every nih-plug CLAP plugin must have.

Safe to run multiple times (idempotent).

All print() calls use plain ASCII only -- no Unicode symbols -- so the script
works on Windows terminals whose code page is cp1252 (or any other narrow
encoding).

LICENSING NOTE
--------------
nih-plug's VST3 bindings link against the Steinberg VST3 SDK (GPLv3).
The VST3 artefact therefore carries GPLv3 obligations; the CLAP artefact is
completely unaffected.  See https://github.com/robbert-vdh/nih-plug#licensing
"""

import re
import sys
from pathlib import Path


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def derive_class_id(struct_name: str) -> str:
    """
    Produce a stable 16-byte ASCII VST3 class ID from the plugin struct name.

    IMPORTANT: Once you have shipped a VST3 plugin publicly this value must
    NEVER change -- hosts use it to match saved states to plugins.
    Hard-code the ID directly in src/lib.rs before your first release if you
    want an ID that is independent of this script's derivation logic.
    """
    seed = (struct_name + "NDE_VST3_ID_XXXXXX")[:16]   # always exactly 16 chars
    return seed


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    lib_rs = Path("src/lib.rs")

    if not lib_rs.exists():
        print("ERROR: src/lib.rs not found. Run this script from the repository root.",
              file=sys.stderr)
        sys.exit(1)

    content = lib_rs.read_text(encoding="utf-8")

    # ---- Idempotency guard -------------------------------------------------
    if "nih_export_vst3!" in content:
        print("INFO: nih_export_vst3! already present in src/lib.rs -- nothing to patch.")
        return

    # ---- Detect plugin struct name -----------------------------------------
    # Every nih-plug CLAP plugin has `impl ClapPlugin for X`.
    match = re.search(r'\bimpl\s+ClapPlugin\s+for\s+(\w+)', content)
    if not match:
        # Fall back to the base Plugin trait.
        match = re.search(r'\bimpl\s+Plugin\s+for\s+(\w+)', content)
    if not match:
        print("ERROR: Cannot find the plugin struct in src/lib.rs.", file=sys.stderr)
        print("       Expected one of:", file=sys.stderr)
        print("         impl ClapPlugin for <StructName> { ... }", file=sys.stderr)
        print("         impl Plugin     for <StructName> { ... }", file=sys.stderr)
        sys.exit(1)

    struct_name = match.group(1)
    class_id    = derive_class_id(struct_name)

    print("patch_vst3.py: struct name  = " + struct_name)
    print("patch_vst3.py: VST3_CLASS_ID = *b\"" + class_id + "\"  (" + str(len(class_id)) + " bytes)")

    # ---- Build the block to append -----------------------------------------
    vst3_block = (
        "\n"
        "// ---------------------------------------------------------------------------\n"
        "// VST3 export -- appended by .github/scripts/patch_vst3.py\n"
        "//\n"
        "// Vst3Plugin is a thin companion trait to Plugin/ClapPlugin.\n"
        "// The two mandatory constants:\n"
        "//\n"
        "//   VST3_CLASS_ID      -- 16-byte unique plugin identifier.\n"
        "//                         NEVER change this after your first public release;\n"
        "//                         hosts use it to match saved presets to plugins.\n"
        "//\n"
        "//   VST3_SUBCATEGORIES -- shown in the host's plug-in browser.\n"
        "//\n"
        "// All other Vst3Plugin constants have sensible defaults.\n"
        "// ---------------------------------------------------------------------------\n"
        "\n"
        "impl Vst3Plugin for " + struct_name + " {\n"
        "    // This ID must stay constant once you have shipped the plugin publicly.\n"
        "    const VST3_CLASS_ID: [u8; 16] = *b\"" + class_id + "\";\n"
        "\n"
        "    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[\n"
        "        Vst3SubCategory::Fx,\n"
        "        Vst3SubCategory::Dynamics,\n"
        "    ];\n"
        "}\n"
        "\n"
        "nih_export_vst3!(" + struct_name + ");\n"
    )

    # Append after the last non-blank line
    patched = content.rstrip("\n") + "\n" + vst3_block
    lib_rs.write_text(patched, encoding="utf-8")

    print("patch_vst3.py: src/lib.rs patched -- VST3 support added for '" + struct_name + "'.")


if __name__ == "__main__":
    main()

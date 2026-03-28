#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
patch_vst3.py
=============
Makes Nebula De-Esser (a nih-plug Rust plugin) produce a VST3 bundle
alongside its existing CLAP bundle.

Two files are patched:

  1. Cargo.toml  -- ensures  features = [..., "vst3"]  is present on the
                    nih_plug dependency line.  This is the root cause of
                    all five compiler errors:
                       cannot find trait `Vst3Plugin` in this scope
                       cannot find macro `nih_export_vst3` in this scope
                       cannot find type `Vst3SubCategory` in this scope
                    Those symbols only exist when nih-plug is compiled with
                    its "vst3" feature flag.  The flag is ON by default, but
                    plugins that specify  default-features = false  (common
                    when selecting a specific GUI back-end such as vizia or
                    egui) accidentally disable it.

  2. src/lib.rs  -- appends  impl Vst3Plugin  +  nih_export_vst3!()  so
                    cargo xtask bundler knows to create a .vst3 bundle.

Both patches are idempotent (safe to run multiple times).
All print() calls use pure ASCII only -- no Unicode -- so the script works
on Windows cp1252 terminals without a UnicodeEncodeError.
"""

import re
import sys
from pathlib import Path


# ===========================================================================
# Part 1 -- Cargo.toml: add "vst3" feature to the nih_plug dependency
# ===========================================================================

def patch_cargo_toml() -> None:
    cargo_toml = Path("Cargo.toml")
    if not cargo_toml.exists():
        print("ERROR: Cargo.toml not found. Run from the repository root.",
              file=sys.stderr)
        sys.exit(1)

    content = cargo_toml.read_text(encoding="utf-8")

    # Idempotency: is "vst3" already in the vicinity of the nih_plug entry?
    nih_pos = content.find("nih_plug")
    if nih_pos != -1:
        neighbourhood = content[nih_pos : nih_pos + 400]
        if '"vst3"' in neighbourhood or "'vst3'" in neighbourhood:
            print("Cargo.toml: nih_plug already has 'vst3' feature -- skipping.")
            return

    patched = _patch_nih_plug_features(content)
    if patched is None:
        print("WARNING: Could not locate nih_plug dependency in Cargo.toml.",
              file=sys.stderr)
        print("         Manually add  features = [\"vst3\"]  to the nih_plug entry.",
              file=sys.stderr)
        return

    cargo_toml.write_text(patched, encoding="utf-8")
    print("Cargo.toml: added \"vst3\" to nih_plug features.")


def _patch_nih_plug_features(content):
    """
    Return the patched Cargo.toml string, or None if nih_plug wasn't found.

    Handles three dependency styles:

      Style A (inline, has features key):
        nih_plug = { git = "...", features = ["egui"], default-features = false }

      Style B (inline, no features key yet):
        nih_plug = { git = "...", default-features = false }

      Style C (TOML table section):
        [dependencies.nih_plug]
        git = "..."
        features = ["egui"]
        default-features = false
    """
    # ---- Style A / B: inline brace block -----------------------------------
    inline_re = re.compile(r'(nih_plug\s*=\s*\{)([^}]*?)(\})', re.DOTALL)
    m = inline_re.search(content)
    if m:
        prefix = m.group(1)
        inner  = m.group(2)
        suffix = m.group(3)
        new_inner = _add_vst3_to_features_inner(inner)
        return content[:m.start()] + prefix + new_inner + suffix + content[m.end():]

    # ---- Style C: [dependencies.nih_plug] table ----------------------------
    table_re = re.compile(r'(\[dependencies\.nih_plug\])(.*?)(?=\n\s*\[|\Z)',
                          re.DOTALL)
    m = table_re.search(content)
    if m:
        header = m.group(1)
        body   = m.group(2)
        feat_re = re.compile(r'(features\s*=\s*\[)([^\]]*?)(\])', re.DOTALL)
        fm = feat_re.search(body)
        if fm:
            new_list = _add_vst3_to_list_text(fm.group(2))
            new_body = (body[:fm.start()] + fm.group(1) +
                        new_list + fm.group(3) + body[fm.end():])
        else:
            new_body = body.rstrip() + '\nfeatures = ["vst3"]\n'
        return content[:m.start()] + header + new_body + content[m.end():]

    return None


def _add_vst3_to_features_inner(inner):
    """
    Given text between { } of an inline dep, ensure features includes "vst3".
    """
    feat_re = re.compile(r'(features\s*=\s*\[)([^\]]*?)(\])', re.DOTALL)
    m = feat_re.search(inner)
    if m:
        new_list = _add_vst3_to_list_text(m.group(2))
        return inner[:m.start()] + m.group(1) + new_list + m.group(3) + inner[m.end():]
    else:
        # No features key yet -- add one before the closing brace
        return inner.rstrip().rstrip(',') + ', features = ["vst3"]'


def _add_vst3_to_list_text(list_inner):
    """
    Given text between [ ] of a features list, add "vst3" if absent.
    """
    if '"vst3"' in list_inner or "'vst3'" in list_inner:
        return list_inner
    stripped = list_inner.rstrip()
    if stripped.endswith(','):
        return stripped + ' "vst3",'
    elif stripped:
        return stripped + ', "vst3"'
    else:
        return '"vst3"'


# ===========================================================================
# Part 2 -- src/lib.rs: append impl Vst3Plugin + nih_export_vst3!()
# ===========================================================================

def derive_class_id(struct_name):
    """
    Produce a stable 16-byte ASCII VST3 class ID from the struct name.

    IMPORTANT: Once you ship the VST3 plugin publicly this value must NEVER
    change -- hosts use it to match saved presets to the plugin.
    Hard-code it directly in lib.rs before your first public release.
    """
    seed = (struct_name + "NDE_VST3_ID_XXXXXX")[:16]
    return seed


def patch_lib_rs() -> None:
    lib_rs = Path("src/lib.rs")
    if not lib_rs.exists():
        print("ERROR: src/lib.rs not found. Run from the repository root.",
              file=sys.stderr)
        sys.exit(1)

    content = lib_rs.read_text(encoding="utf-8")

    if "nih_export_vst3!" in content:
        print("src/lib.rs: nih_export_vst3! already present -- skipping.")
        return

    # Detect plugin struct name from the mandatory ClapPlugin impl
    match = re.search(r'\bimpl\s+ClapPlugin\s+for\s+(\w+)', content)
    if not match:
        match = re.search(r'\bimpl\s+Plugin\s+for\s+(\w+)', content)
    if not match:
        print("ERROR: Cannot find the plugin struct in src/lib.rs.", file=sys.stderr)
        print("       Expected:  impl ClapPlugin for <StructName> { ... }", file=sys.stderr)
        sys.exit(1)

    struct_name = match.group(1)
    class_id    = derive_class_id(struct_name)

    print("src/lib.rs: struct name   = " + struct_name)
    print("src/lib.rs: VST3_CLASS_ID = *b\"" + class_id + "\"")

    vst3_block = (
        "\n"
        "// ---------------------------------------------------------------------------\n"
        "// VST3 export -- appended by .github/scripts/patch_vst3.py\n"
        "//\n"
        "//   VST3_CLASS_ID      -- 16-byte unique plugin identifier.\n"
        "//                         NEVER change after first public VST3 release.\n"
        "//   VST3_SUBCATEGORIES -- shown in the host's plug-in browser.\n"
        "// ---------------------------------------------------------------------------\n"
        "\n"
        "impl Vst3Plugin for " + struct_name + " {\n"
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

    patched = content.rstrip("\n") + "\n" + vst3_block
    lib_rs.write_text(patched, encoding="utf-8")
    print("src/lib.rs: VST3 export added for '" + struct_name + "'.")


# ===========================================================================
# Entry point
# ===========================================================================

if __name__ == "__main__":
    print("--- patch_vst3.py: patching Cargo.toml ---")
    patch_cargo_toml()
    print("--- patch_vst3.py: patching src/lib.rs ---")
    patch_lib_rs()
    print("--- patch_vst3.py: done ---")

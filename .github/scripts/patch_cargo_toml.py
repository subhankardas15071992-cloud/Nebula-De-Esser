#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
patch_cargo_toml.py
===================
Ensures the root Cargo.toml treats this project as a Cargo workspace that
includes "xtask" as a member.  This is required so that the `.cargo/config.toml`
alias  `xtask = "run --package xtask --"`  can resolve the xtask crate.

The script handles two common starting states:

  Case A  Single-crate Cargo.toml (no [workspace] section)
          --> Inserts  [workspace]\nmembers = [".", "xtask"]\n\n
              before the first [package] or [lib] section.

  Case B  Already a workspace Cargo.toml
          --> Appends "xtask" to the existing members list (if not already there).

Safe to run multiple times (idempotent).

All output uses plain ASCII so it works on Windows cp1252 terminals.
"""

import re
import sys
from pathlib import Path


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def find_workspace_members_span(text: str):
    """
    Locate the members = [...] value inside a [workspace] section.
    Returns (start_of_bracket, end_of_bracket) character indices, or None.
    """
    # Find the [workspace] header
    ws_match = re.search(r'^\[workspace\]', text, re.MULTILINE)
    if ws_match is None:
        return None

    # From that point, find  members = [...]
    # The list may span multiple lines.
    after_ws = ws_match.end()
    members_match = re.search(
        r'\bmembers\s*=\s*(\[.*?\])',
        text[after_ws:],
        re.DOTALL,
    )
    if members_match is None:
        return None

    abs_start = after_ws + members_match.start(1)
    abs_end   = after_ws + members_match.end(1)
    return abs_start, abs_end


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    cargo_toml = Path("Cargo.toml")

    if not cargo_toml.exists():
        print("ERROR: Cargo.toml not found. Run from the repository root.", file=sys.stderr)
        sys.exit(1)

    original = cargo_toml.read_text(encoding="utf-8")

    # ---- Idempotency guard ------------------------------------------------
    if '"xtask"' in original or "'xtask'" in original:
        print("OK  Cargo.toml already has 'xtask' in workspace members -- nothing to do.")
        return

    # ---- Case B: [workspace] already exists --------------------------------
    span = find_workspace_members_span(original)
    if span is not None:
        start, end = span
        bracket_content = original[start:end]  # e.g.  [".", "some-plugin"]

        # Insert "xtask" before the closing ]
        closing = bracket_content.rindex(']')
        # Decide separator based on whether the list is multi-line
        if '\n' in bracket_content:
            separator = ',\n    '
            # Trim trailing whitespace/commas before ]
            inner = bracket_content[1:closing].rstrip().rstrip(',')
            new_bracket = '[' + inner + ',\n    "xtask",\n]'
        else:
            inner = bracket_content[1:closing].rstrip().rstrip(',')
            new_bracket = '[' + inner + ', "xtask"]'

        patched = original[:start] + new_bracket + original[end:]
        cargo_toml.write_text(patched, encoding="utf-8")
        print("OK  Added \"xtask\" to existing [workspace] members in Cargo.toml.")
        return

    # ---- Case A: no [workspace] yet ----------------------------------------
    # Prepend a [workspace] block before the first section header.
    workspace_block = '[workspace]\nmembers = [".", "xtask"]\n\n'

    first_section = re.search(r'^\[', original, re.MULTILINE)
    if first_section:
        insert_at = first_section.start()
        patched = original[:insert_at] + workspace_block + original[insert_at:]
    else:
        # No sections at all (unusual) — just prepend
        patched = workspace_block + original

    cargo_toml.write_text(patched, encoding="utf-8")
    print("OK  Added [workspace] with members = [\".\", \"xtask\"] to Cargo.toml.")


if __name__ == "__main__":
    main()

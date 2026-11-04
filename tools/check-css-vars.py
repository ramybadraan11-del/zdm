#!/usr/bin/env python3
"""List CSS custom properties that are referenced but never defined.

A missing variable does not error in CSS — the declaration is simply dropped and
the element falls back to an inherited or initial value. That is how the floating
prompt window silently lost its text colour after the token rewrite, so this is
checked on every preflight run.
"""

import re
import sys
from pathlib import Path

CSS = Path("apps/desktop/src/styles/index.css")

# `--i` is set per-row from JavaScript (animation stagger), not in the stylesheet.
RUNTIME_ONLY = {"--i"}


def main() -> int:
    try:
        source = CSS.read_text(encoding="utf-8")
    except OSError as error:
        print(f"cannot read {CSS}: {error}", file=sys.stderr)
        return 1

    used = set(re.findall(r"var\((--[a-z0-9-]+)", source))
    defined = set(re.findall(r"^\s*(--[a-z0-9-]+)\s*:", source, re.MULTILINE))

    missing = sorted(used - defined - RUNTIME_ONLY)
    if missing:
        print(" ".join(missing))
    return 0


if __name__ == "__main__":
    sys.exit(main())

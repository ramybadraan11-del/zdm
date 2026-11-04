#!/usr/bin/env python3
"""Generate the NSIS wizard bitmaps from the brand mark.

Developed by Ramy Badraan

MUI2 expects BMP files with exact dimensions:
  * header.bmp  — 150x57, the strip on every page header
  * wizard.bmp  — 164x314, the panel on the welcome/finish pages

ImageMagick is used when available; otherwise the script explains what to drop
in place manually rather than producing something broken.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICON_SRC = ROOT / "assets" / "icon-256.png"
OUT = ROOT / "windows" / "icons"

BG = "#0B1120"
ACCENT = "#38BDF8"


def magick() -> str | None:
    for name in ("magick", "convert"):
        found = shutil.which(name)
        if found:
            return found
    return None


def main() -> int:
    if not ICON_SRC.exists():
        print(f"missing {ICON_SRC}; run tools/build-icons.sh first", file=sys.stderr)
        return 1

    tool = magick()
    if not tool:
        print("ImageMagick not found: generate header.bmp and wizard.bmp manually.", file=sys.stderr)
        return 1

    OUT.mkdir(parents=True, exist_ok=True)

    # Header strip: the mark on the left, brand gradient behind it.
    subprocess.run(
        [
            tool,
            "-size", "150x57",
            f"gradient:{BG}-{ACCENT}",
            "(", str(ICON_SRC), "-resize", "40x40", ")",
            "-gravity", "west", "-geometry", "+8+0", "-composite",
            "-type", "TrueColor",
            "-depth", "8",
            # MUI only accepts the classic BMP3 layout, not the newer BITMAPV4
            # header ImageMagick produces by default.
            "-compress", "none",
            f"BMP3:{OUT / 'header.bmp'}",
        ],
        check=True,
    )

    # Wizard panel: vertical brand panel with the mark centred.
    subprocess.run(
        [
            tool,
            "-size", "164x314",
            f"gradient:#050A14-{BG}",
            "(", str(ICON_SRC), "-resize", "96x96", ")",
            "-gravity", "north", "-geometry", "+0+70", "-composite",
            "-type", "TrueColor",
            "-depth", "8",
            "-compress", "none",
            f"BMP3:{OUT / 'wizard.bmp'}",
        ],
        check=True,
    )

    for name in ("header.bmp", "wizard.bmp"):
        path = OUT / name
        print(f"  {name}: {path.stat().st_size:,} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())

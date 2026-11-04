#!/usr/bin/env python3
"""Build the two browser packages from one source tree.

Developed by Ramy Badraan

Chromium and Firefox need slightly different manifests, and shipping one file for
both produces warnings that AMO shows to the reviewer:

  * Chromium (MV3) needs `background.service_worker`.
  * Firefox needs `background.scripts`; it ignores `service_worker` and warns
    about it on every submission.

So the shared source stays Chromium-shaped, and the Firefox copy is derived:

    dist/<id>.crx              Chromium, unchanged source
    dist/zdm@zyzoom.com.xpi    Firefox, service_worker removed, scripts kept

The XPI is a plain zip (exactly what AMO expects when you upload a file) and the
CRX3 is signed so Chromium can install it from the `external_crx` channel.

Run: python3 tools/build-packages.py
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "apps" / "extension"
ASSETS = ROOT / "assets"
WORK = ROOT / ".build" / "packages"

SKIP_NAMES = {"manifest.json"}  # handled explicitly
SKIP_SUFFIXES = {".crx", ".xpi", ".pem", ".zip", ".map"}


def collect(source: Path) -> list[Path]:
    """Every file that belongs in a package, manifest first."""
    files: list[Path] = []
    for path in source.rglob("*"):
        if not path.is_file():
            continue
        relative = path.relative_to(source)
        if any(part in {"node_modules", ".git"} for part in relative.parts):
            continue
        if path.suffix in SKIP_SUFFIXES:
            continue
        files.append(path)
    files.sort(key=lambda p: (p.name != "manifest.json", str(p)))
    return files


def write_zip(target: Path, files: list[Path], source_root: Path, manifest: dict) -> None:
    target.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(target, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        archive.writestr(
            "manifest.json", json.dumps(manifest, indent=2, ensure_ascii=False) + "\n"
        )
        for path in files:
            if path.name == "manifest.json":
                continue
            archive.write(path, path.relative_to(source_root).as_posix())


def build_firefox(chromium_manifest: dict, files: list[Path]) -> Path:
    """Firefox build: no service_worker, explicit Android exclusion."""
    manifest = json.loads(json.dumps(chromium_manifest))  # deep copy

    background = manifest.get("background", {})
    background.pop("service_worker", None)
    # Firefox needs an event page; `scripts` is already present in the source.
    if "scripts" not in background:
        background["scripts"] = ["background.js"]
    manifest["background"] = background

    target = ASSETS / f"{chromium_manifest['browser_specific_settings']['gecko']['id']}.xpi"
    write_zip(target, files, SOURCE, manifest)
    return target


def main() -> int:
    if not (SOURCE / "manifest.json").is_file():
        print(f"no manifest.json in {SOURCE}", file=sys.stderr)
        return 1

    chromium_manifest = json.loads((SOURCE / "manifest.json").read_text(encoding="utf-8"))
    files = collect(SOURCE)
    ASSETS.mkdir(parents=True, exist_ok=True)
    WORK.mkdir(parents=True, exist_ok=True)

    # ---- Firefox: derived manifest, plain zip named .xpi -------------------
    xpi = build_firefox(chromium_manifest, files)
    print(f"  firefox : {xpi.relative_to(ROOT)}  ({xpi.stat().st_size:,} bytes)")

    # ---- Chromium: signed CRX3 from the unmodified source ------------------
    crx_script = ROOT / "tools" / "build-crx.py"
    result = subprocess.run(
        [
            sys.executable,
            str(crx_script),
            "--source", str(SOURCE),
            "--output", str(ASSETS / "zdm.crx"),
            "--key", str(ROOT / "packaging" / "keys" / "zdm-extension.pem"),
            "--xpi", str(WORK / "unused.xpi"),
            "--manifest", str(SOURCE / "manifest.json"),
        ],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        print(result.stdout, file=sys.stderr)
        print(result.stderr, file=sys.stderr)
        print("CRX3 signing failed", file=sys.stderr)
        return 1

    print(result.stdout.strip())

    # The signer also writes an XPI; remove that one, the Firefox build above is
    # the canonical package (it has the Firefox-shaped manifest).
    unused = WORK / "unused.xpi"
    if unused.exists():
        unused.unlink()

    return 0


if __name__ == "__main__":
    sys.exit(main())

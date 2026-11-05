#!/usr/bin/env bash
# ==============================================================================
# ZDM — Windows installer build (from Linux or Windows)
# Developed by Ramy Badraan
#
# Cross-compiles zdm.exe + zdm-host.exe with cargo-xwin (MSVC target, no Windows
# machine required) and produces the full NSIS installer:
#
#   dist/Zyzoom_Download_Manager_Setup_<version>.exe
#
# Requirements
#   • cargo-xwin          cargo install cargo-xwin --locked
#   • makensis            built below from source, or system NSIS
#   • 7z / node / npm     for packaging helpers
#
# On Windows, prefer windows\build-installer.ps1, which uses the native toolchain.
#
# Usage: bash windows/build_installer.sh [--skip-build] [--skip-nsis]
# ==============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(python3 -c "import json;print(json.load(open('apps/desktop/src-tauri/tauri.conf.json'))['version'])")"
# Which architectures to build: "x64", "x86" or "both" (default).
ARCH_MODE="${ZDM_ARCH:-both}"
OUT="dist"
WORK="${ROOT}/.build/win"
NSIS_HOME="${ROOT}/.build/nsis"

SKIP_BUILD=0
SKIP_NSIS=0
for arg in "$@"; do
  case "$arg" in
    --skip-build) SKIP_BUILD=1 ;;
    --skip-nsis)  SKIP_NSIS=1 ;;
    *) echo "Unknown option: $arg" >&2; exit 1 ;;
  esac
done

CYAN=$'\033[0;36m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'
BOLD=$'\033[1m'; DIM=$'\033[2m'; NC=$'\033[0m'
step() { printf '\n%s\n' "${BOLD}${CYAN}▸ $*${NC}"; }
ok()   { printf '  %s %s\n' "${GREEN}✓${NC}" "$*"; }
warn() { printf '  %s %s\n' "${YELLOW}!${NC}" "$*"; }
die()  { printf '  %s %s\n' "${YELLOW}✗${NC}" "$*" >&2; exit 1; }

mkdir -p "$OUT" "$WORK"

# ------------------------------------------------------------------ toolchain
step "Checking the cross toolchain"

command -v cargo >/dev/null 2>&1 || die "cargo not found"
command -v node  >/dev/null 2>&1 || die "node not found"

if ! cargo xwin --version >/dev/null 2>&1; then
  die "cargo-xwin is missing. Install it with: cargo install cargo-xwin --locked"
fi
ok "cargo-xwin $(cargo xwin --version 2>/dev/null | head -1)"

for target in x86_64-pc-windows-msvc i686-pc-windows-msvc; do
  if [ "$ARCH_MODE" = "x64" ] && [ "$target" != "x86_64-pc-windows-msvc" ]; then continue; fi
  if [ "$ARCH_MODE" = "x86" ] && [ "$target" != "i686-pc-windows-msvc" ]; then continue; fi
  rustup target list --installed | grep -qx "$target" || {
    die "the $target target is missing. Install it with: rustup target add $target"
  }
  ok "target $target installed"
done

[ -f apps/desktop/node_modules/.package-lock.json ] || npm --prefix apps/desktop install --no-audit --no-fund

# -------------------------------------------------------------------- binaries
if [ "$SKIP_BUILD" -eq 0 ]; then
  step "Building the frontend"
  npm --prefix apps/desktop run build
  ok "apps/desktop/dist"

  step "Cross-compiling the Windows binaries"

  case "$ARCH_MODE" in
    x64|both)
      cargo xwin build --release --target x86_64-pc-windows-msvc -p zdm -p zdm-host
      ok "x86_64: zdm.exe + zdm-host.exe"
      ;;
  esac

  case "$ARCH_MODE" in
    x86|both)
      # `cargo xwin` will not splat an x86-only cache, so the 32-bit build goes
      # through the dedicated script that drives cargo directly.
      bash tools/build-windows-x86.sh -p zdm -p zdm-host \
        || die "the 32-bit build failed; see the output above"
      ok "i686: zdm.exe + zdm-host.exe"
      ;;
  esac
fi

# Validate every architecture we are about to package.
verify_arch() {
  local triple="$1" want_machine="$2" label="$3"
  local exe="target/$triple/release/zdm.exe"
  local host="target/$triple/release/zdm-host.exe"

  [ -f "$exe" ]  || die "$exe is missing; run without --skip-build"
  [ -f "$host" ] || die "$host is missing; run without --skip-build"

  python3 - "$exe" "$host" "$want_machine" "$label" <<'PYVERIFY'
import struct, sys

exe, host, want_hex, label = sys.argv[1:5]
want = int(want_hex, 16)

for path, expected_subsystem, name in ((exe, 2, "zdm.exe"), (host, 3, "zdm-host.exe")):
    data = open(path, "rb").read()
    if data[:2] != b"MZ":
        sys.exit(f"{label} {name}: not an MZ executable")
    pe = struct.unpack("<I", data[0x3C:0x40])[0]
    if data[pe:pe + 4] != b"PE\0\0":
        sys.exit(f"{label} {name}: no PE header")

    machine = struct.unpack("<H", data[pe + 4:pe + 6])[0]
    subsystem = struct.unpack("<H", data[pe + 0x5C:pe + 0x5E])[0]

    if machine != want:
        sys.exit(f"{label} {name}: machine 0x{machine:04x}, expected 0x{want:04x}")
    if subsystem != expected_subsystem:
        sys.exit(f"{label} {name}: subsystem {subsystem}, expected {expected_subsystem}")

    kind = "PE32+" if machine == 0x8664 else "PE32"
    arch = "x86-64" if machine == 0x8664 else "i386"
    print(f"  {label} {name}: {kind} {arch}, subsystem={subsystem}")
PYVERIFY
  ok "$label binaries verified"
}

case "$ARCH_MODE" in
  x64|both) verify_arch x86_64-pc-windows-msvc 8664 "64-bit" ;;
esac
case "$ARCH_MODE" in
  x86|both) verify_arch i686-pc-windows-msvc 014c "32-bit" ;;
esac

# ------------------------------------------------------------------- extension
step "Packaging the browser extension"
if python3 -c "import cryptography" 2>/dev/null; then
  python3 tools/build-packages.py >/dev/null && ok "CRX3 signed + Firefox XPI built"
else
  warn "python-cryptography missing: reusing existing assets"
fi

# The installer reads both artefacts from dist/.
CRX_ID="jgoccphfikadefpdemmaohkpbomgjcak"
for file in "assets/$CRX_ID.crx" "assets/zdm@zyzoom.com.xpi"; do
  [ -f "$file" ] && cp "$file" "$OUT/" && ok "$(basename "$file")"
done

# --------------------------------------------------------------------- helpers
step "Bundling helpers"
if [ ! -f "$OUT/yt-dlp.exe" ]; then
  if curl -fsSL --max-time 180 -o "$OUT/yt-dlp.exe" \
      "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"; then
    ok "yt-dlp.exe ($(du -h "$OUT/yt-dlp.exe" | cut -f1))"
  else
    warn "could not fetch yt-dlp.exe; video download will self-heal at runtime"
  fi
else
  ok "yt-dlp.exe (cached)"
fi

# ffmpeg: required to merge separate video/audio streams and to encode mp3.
# Without it every YouTube download above 720p lands as two fragments.
if [ ! -f "$OUT/ffmpeg.exe" ]; then
  printf '  %s\n' "${DIM}Fetching ffmpeg (one time)…${NC}"
  if curl -fsSL --max-time 600 -o "$WORK/ffmpeg.zip" \
      "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"; then
    if unzip -oj -q "$WORK/ffmpeg.zip" "*/bin/ffmpeg.exe" -d "$OUT" 2>/dev/null; then
      ok "ffmpeg.exe ($(du -h "$OUT/ffmpeg.exe" | cut -f1))"
    else
      warn "could not extract ffmpeg.exe from the archive"
    fi
    rm -f "$WORK/ffmpeg.zip"
  else
    die "ffmpeg.exe is required for video downloads and could not be downloaded.
       Fetch it manually and place it at dist/ffmpeg.exe:
         https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip  (bin/ffmpeg.exe)"
  fi
else
  ok "ffmpeg.exe (cached)"
fi

# A 0-byte or truncated file would install cleanly and break at runtime.
FFMPEG_SIZE=$(stat -c%s "$OUT/ffmpeg.exe" 2>/dev/null || echo 0)
if [ "$FFMPEG_SIZE" -lt 5000000 ]; then
  die "dist/ffmpeg.exe is only ${FFMPEG_SIZE} bytes; it is truncated. Delete it and rebuild."
fi

# WebView2 bootstrapper: bundled by default, but the *small* one.
#
# linkid=2124701 serves the 205 MB offline runtime installer, which tripled the
# package size for no benefit: the 1.8 MB bootstrapper downloads the same runtime
# silently during setup on the machines that actually need it. Windows 11 already
# has the runtime, so in practice it is never fetched.
if [ "${ZDM_NO_WEBVIEW2:-0}" != "1" ]; then
  if [ ! -f "$OUT/MicrosoftEdgeWebView2Setup.exe" ]; then
    if curl -fsSL --max-time 180 -o "$OUT/MicrosoftEdgeWebView2Setup.exe" \
        "https://go.microsoft.com/fwlink/?linkid=2124703"; then
      SIZE_KB=$(( $(stat -c%s "$OUT/MicrosoftEdgeWebView2Setup.exe") / 1024 ))
      ok "WebView2 bootstrapper bundled (${SIZE_KB} KB)"
    else
      warn "could not fetch the WebView2 bootstrapper; the installer will link to Microsoft instead"
    fi
  else
    ok "WebView2 bootstrapper (cached)"
  fi

  # Guard against the old 205 MB offline installer lingering in dist/.
  SIZE_BYTES=$(stat -c%s "$OUT/MicrosoftEdgeWebView2Setup.exe" 2>/dev/null || echo 0)
  if [ "$SIZE_BYTES" -gt 20000000 ]; then
    warn "WebView2 file is $((SIZE_BYTES/1024/1024)) MB (offline installer); replacing with the bootstrapper"
    rm -f "$OUT/MicrosoftEdgeWebView2Setup.exe"
    curl -fsSL --max-time 180 -o "$OUT/MicrosoftEdgeWebView2Setup.exe" \
      "https://go.microsoft.com/fwlink/?linkid=2124703" || true
  fi
else
  printf '  %s\n' "${DIM}WebView2 not bundled (ZDM_NO_WEBVIEW2=1)${NC}"
  rm -f "$OUT/MicrosoftEdgeWebView2Setup.exe"
fi

# ----------------------------------------------------------------------- NSIS
if [ "$SKIP_NSIS" -eq 1 ]; then
  printf '\n%s\n' "${DIM}Skipping the installer (--skip-nsis)${NC}"
  exit 0
fi

step "Preparing NSIS"

MAKENSIS="$(command -v makensis || true)"

if [ -z "$MAKENSIS" ]; then
  # Build makensis from source: the binary distribution is Windows-only and Wine
  # is not guaranteed to be installed.
  if [ ! -x "$NSIS_HOME/bin/makensis" ]; then
    printf '%s\n' "  ${DIM}makensis not found; building it from source (one time)${NC}"

    command -v g++ >/dev/null 2>&1 || die "g++ is required to build makensis"

    if [ ! -d "$NSIS_HOME/src" ]; then
      mkdir -p "$NSIS_HOME"
      curl -fsSL --max-time 300 -o "$WORK/nsis.tar.bz2" \
        "https://downloads.sourceforge.net/project/nsis/NSIS%203/3.11/nsis-3.11-src.tar.bz2" \
        || die "could not download the NSIS sources"
      tar -xjf "$WORK/nsis.tar.bz2" -C "$WORK"
      mv "$WORK/nsis-3.11-src" "$NSIS_HOME/src"
      curl -fsSL --max-time 300 -o "$WORK/nsis.zip" \
        "https://downloads.sourceforge.net/project/nsis/NSIS%203/3.11/nsis-3.11.zip" \
        || die "could not download the NSIS distribution (needed for stubs/headers)"
      unzip -q -o "$WORK/nsis.zip" -d "$WORK"
    fi

    # SCons ships as a pip package; use an isolated venv so the system Python is
    # untouched (PEP 668 distributions refuse global installs).
    if [ ! -x "$NSIS_HOME/scons/bin/scons" ]; then
      python3 -m venv "$NSIS_HOME/scons"
      "$NSIS_HOME/scons/bin/pip" install --quiet scons
    fi

    (
      cd "$NSIS_HOME/src"
      # The stubs are 32-bit Windows binaries that need MinGW headers; we reuse
      # the prebuilt ones from the official distribution instead.
      rm -rf build
      "$NSIS_HOME/scons/bin/scons" -j"$(nproc)" \
        SKIPSTUBS=bzip2,lzma,zlib SKIPPLUGINS=all SKIPUTILS=all SKIPDOC=all \
        >"$WORK/nsis-build.log" 2>&1 || {
          tail -20 "$WORK/nsis-build.log" >&2
          die "building makensis failed (see $WORK/nsis-build.log)"
        }
    )

    mkdir -p "$NSIS_HOME/bin"
    cp "$NSIS_HOME/src/build/urelease/makensis/makensis" "$NSIS_HOME/bin/makensis"
    ok "makensis built from source"
  fi

  # The compiler needs Include/, Contrib/ and Stubs/ at runtime, located through
  # NSISDIR. The Windows distribution provides the stubs.
  if [ ! -d "$NSIS_HOME/share/Stubs" ] || [ ! -d "$NSIS_HOME/share/Contrib" ]; then
    mkdir -p "$NSIS_HOME/share"
    for dir in Stubs Include Contrib Plugins; do
      [ -d "$WORK/nsis-3.11/$dir" ] && cp -r "$WORK/nsis-3.11/$dir" "$NSIS_HOME/share/"
    done
  fi

  MAKENSIS="$NSIS_HOME/bin/makensis"
  export NSISDIR="$NSIS_HOME/share"
fi

ok "makensis: $MAKENSIS"

# The add-on version comes from its manifest; the installer writes it into Chromium's
# external_version, and a mismatch there means the extension is never refreshed.
EXT_VERSION="$(python3 -c "import json;print(json.load(open('apps/extension/manifest.json'))['version'])" 2>/dev/null || true)"
[ -n "$EXT_VERSION" ] || die "could not read the extension version from apps/extension/manifest.json"
ok "extension payload version: $EXT_VERSION"

step "Compiling the installers"

build_installer() {
  local arch="$1" triple="$2" suffix="$3"

  ( cd windows
    "$MAKENSIS" -V2 \
      -DWIN_ARCH="$arch" \
      -DWIN_RELEASE_DIR="../target/$triple/release" \
      -DEXT_VERSION="$EXT_VERSION" \
      installer.nsi
  )

  local out="$OUT/Zyzoom_Download_Manager_Setup_${VERSION}_${suffix}.exe"
  [ -f "$out" ] || die "the $suffix installer was not produced"
  ok "$(basename "$out") — $(du -h "$out" | cut -f1)"
}

case "$ARCH_MODE" in
  x64|both) build_installer x64 x86_64-pc-windows-msvc x64 ;;
esac
case "$ARCH_MODE" in
  x86|both) build_installer x86 i686-pc-windows-msvc x86 ;;
esac

# The most recently built installer is the one summarised below.
INSTALLER="$(ls -t "$OUT"/Zyzoom_Download_Manager_Setup_"${VERSION}"_*.exe 2>/dev/null | head -1)"
[ -n "$INSTALLER" ] || die "no installer was produced"

# ------------------------------------------------------------------ validation
step "Validating the installer"

python3 - "$INSTALLER" <<'PY'
import struct, sys

path = sys.argv[1]
data = open(path, "rb").read()
if data[:2] != b"MZ":
    sys.exit("installer: not an MZ executable")
pe = struct.unpack("<I", data[0x3C:0x40])[0]
if data[pe:pe + 4] != b"PE\0\0":
    sys.exit("installer: no PE header")
subsystem = struct.unpack("<H", data[pe + 0x5C:pe + 0x5E])[0]
if subsystem != 2:
    sys.exit(f"installer: subsystem {subsystem}, expected GUI")
print(f"  PE32 GUI installer, {len(data):,} bytes")
PY

if command -v 7z >/dev/null 2>&1; then
  COUNT="$(7z l "$INSTALLER" | grep -cE "\.(exe|dll|js|json|css|html|png|ico|crx|xpi|bmp)$" || true)"
  ok "$COUNT payload entries readable"
  if 7z l "$INSTALLER" | grep -q "yt-dlp.exe"; then
    ok "yt-dlp.exe is bundled"
  else
    warn "yt-dlp.exe is not bundled"
  fi
  if 7z l "$INSTALLER" | grep -q "\.crx"; then
    ok "signed CRX is bundled"
  else
    warn "no CRX: Chromium auto-install will be unavailable"
  fi
else
  warn "7z not found: skipping payload inspection"
fi

SHA="$(sha256sum "$INSTALLER" | cut -d' ' -f1)"

printf '\n%s\n' "${GREEN}${BOLD}  Installer ready${NC}"
printf '  %s\n' "$(du -h "$INSTALLER" | cut -f1)  ${DIM}$INSTALLER${NC}"
printf '  %s\n' "sha256: ${DIM}$SHA${NC}"
printf '\n'

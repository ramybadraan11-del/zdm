#!/usr/bin/env bash
# ==============================================================================
# ZDM Icon Factory
# Developed by Ramy Badraan
#
# Renders every icon the build needs from one master SVG so the mark can never
# drift between the app, the tray, the browser extension and the installers.
#
# Usage: tools/build-icons.sh [--force]
# ==============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${ROOT}/assets/logo.svg"
APP_ICONS="${ROOT}/apps/desktop/src-tauri/icons"
EXT_ICONS="${ROOT}/apps/extension/icons"
ASSETS="${ROOT}/assets"
WIN_ICONS="${ROOT}/windows/icons"

RENDER_DIR="${ROOT}/.preview"
mkdir -p "$RENDER_DIR"

command -v rsvg-convert >/dev/null 2>&1 || {
  echo "rsvg-convert (librsvg) is required to build icons." >&2
  echo "  Arch:   sudo pacman -S librsvg" >&2
  echo "  Debian: sudo apt install librsvg2-bin" >&2
  echo "  Fedora: sudo dnf install librsvg2-tools" >&2
  exit 1
}

render() {
  local size="$1" out="$2"
  rsvg-convert -w "$size" -h "$size" -b none "$SRC" -o "$out"
  echo "  ${size}px -> ${out#${ROOT}/}"
}

echo ">> Rendering PNG icon set from assets/logo.svg"
mkdir -p "$APP_ICONS" "$EXT_ICONS" "$WIN_ICONS" \
  "${ASSETS}/icons/16x16/apps" "${ASSETS}/icons/32x32/apps" \
  "${ASSETS}/icons/48x48/apps" "${ASSETS}/icons/64x64/apps" \
  "${ASSETS}/icons/128x128/apps" "${ASSETS}/icons/256x256/apps" \
  "${ASSETS}/icons/512x512/apps" "${ASSETS}/icons/scalable/apps"

for size in 16 32 48 64 128 256 512; do
  render "$size" "${ASSETS}/icons/${size}x${size}/apps/zdm.png"
done

# Stable, referenced by packaging scripts and the docs.
for size in 32 64 128 256 512 1024; do
  render "$size" "${ASSETS}/icon-${size}.png"
done

# Tauri needs these exact names.
render 32  "${APP_ICONS}/32x32.png"
render 128 "${APP_ICONS}/128x128.png"
render 128 "${APP_ICONS}/128x128@2x.png"
render 512 "${APP_ICONS}/icon.png"

# Browser extension icon set.
for size in 16 32 48 128; do
  render "$size" "${EXT_ICONS}/icon${size}.png"
done

# Scalable copy for desktop environments that prefer SVG.
cp "$SRC" "${ASSETS}/icons/scalable/apps/zdm.svg"

# ---------------------------------------------------------------- Windows .ico
echo ">> Building Windows .ico"
if command -v magick >/dev/null 2>&1; then
  MAGICK="magick"
elif command -v convert >/dev/null 2>&1; then
  MAGICK="convert"
else
  MAGICK=""
fi

if [ -n "$MAGICK" ]; then
  # Multi-resolution ICO: 16/24/32/48/64/128/256 keeps Explorer and the taskbar
  # crisp at every size Windows asks for.
  TMP_ICO_DIR="${RENDER_DIR}/ico"
  mkdir -p "$TMP_ICO_DIR"
  for size in 16 24 32 48 64 128 256; do
    render "$size" "${TMP_ICO_DIR}/icon-${size}.png"
  done
  "$MAGICK" "${TMP_ICO_DIR}"/icon-16.png "${TMP_ICO_DIR}"/icon-24.png \
    "${TMP_ICO_DIR}"/icon-32.png "${TMP_ICO_DIR}"/icon-48.png \
    "${TMP_ICO_DIR}"/icon-64.png "${TMP_ICO_DIR}"/icon-128.png \
    "${TMP_ICO_DIR}"/icon-256.png "${APP_ICONS}/icon.ico"
  cp "${APP_ICONS}/icon.ico" "${WIN_ICONS}/icon.ico"
  cp "${APP_ICONS}/icon.ico" "${ASSETS}/icon.ico"
  echo "  .ico written to apps/desktop/src-tauri/icons, windows/icons, assets"
else
  echo "  ImageMagick not found: .ico step skipped (PNG icons are already written)."
fi

echo ">> Icon build complete."

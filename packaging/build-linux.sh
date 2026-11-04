#!/usr/bin/env bash
# ==============================================================================
# ZDM Linux Packaging
# Developed by Ramy Badraan
#
# Builds three native packages from one release binary:
#   • .deb  (Debian, Ubuntu, Mint, Pop!_OS, Kali, Zorin…)
#   • .rpm  (Fedora, RHEL, Rocky, Alma, openSUSE)
#   • .AppImage (any distro, no root, single file)
#
# Everything is assembled by hand so the packages work without network access at
# build time, and so post-install hooks can wire autostart + browsers correctly
# (the generic Tauri bundles never do that).
#
# Usage: bash packaging/build-linux.sh [--skip-build] [--only deb|rpm|appimage]
# ==============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(python3 -c "import json;print(json.load(open('apps/desktop/src-tauri/tauri.conf.json'))['version'])")"
PKG_NAME="zyzoom-download-manager"
BIN_NAME="zdm"
HOST_NAME="zdm-host"
APP_ID="com.zyzoom.zdm"
MAINTAINER="Ramy Badraan"
DESCRIPTION="Multi-segment download accelerator with browser capture and video download"
HOMEPAGE="https://zyzoom.net"
EXT_CHROME_ID="jgoccphfikadefpdemmaohkpbomgjcak"
EXT_GECKO_ID="zdm@zyzoom.com"

OUT="${ROOT}/dist"
STAGE="${ROOT}/.build/pkg"
mkdir -p "$OUT"

SKIP_BUILD=0
ONLY=""

# Kept in sync with packaging/linux/zdm.spec. The direct RPM writer needs these
# because there is no rpmbuild to read the spec on Arch/Debian build hosts.
RPM_POST_SCRIPT='if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || :
fi'
RPM_PREUN_SCRIPT='if [ "$1" = 0 ]; then
  systemctl --user stop zdm.service >/dev/null 2>&1 || :
  pkill -x zdm >/dev/null 2>&1 || :
  pkill -x zdm-host >/dev/null 2>&1 || :
fi'
RPM_POSTUN_SCRIPT='if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || :
fi'

while [ $# -gt 0 ]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1 ;;
    --only) shift; ONLY="${1:-}" ;;
    *) echo "Unknown option: $1" >&2; exit 1 ;;
  esac
  shift
done

CYAN=$'\033[0;36m'; GREEN=$'\033[0;32m'; YELLOW=$'\033[1;33m'; BOLD=$'\033[1m'; DIM=$'\033[2m'; NC=$'\033[0m'
step() { printf '\n%s\n' "${BOLD}${CYAN}▸ $*${NC}"; }
ok()   { printf '%s\n' "  ${GREEN}✓${NC} $*"; }
warn() { printf '%s\n' "  ${YELLOW}!${NC} $*"; }

# Write the Omaha update manifest Chromium expects at the other end of
# `update_url`. Pointing that policy at a .crx makes Chrome parse a ZIP as XML
# and discard the whole policy, so the extension never installs.
write_update_manifest() {
  local target="$1" codebase="$2"
  local version
  version="$(python3 -c "import json;print(json.load(open('apps/extension/manifest.json'))['version'])" 2>/dev/null || echo 1.0.0)"
  mkdir -p "$(dirname "$target")"
  printf '%s\n' \
    '<?xml version="1.0" encoding="UTF-8"?>' \
    '<gupdate xmlns="http://www.google.com/update2/response" protocol="2.0">' \
    "  <app appid=\"${EXT_CHROME_ID}\">" \
    "    <updatecheck codebase=\"${codebase}\" version=\"${version}\" />" \
    '  </app>' \
    '</gupdate>' >"$target"
}

# ------------------------------------------------------------------ preflight
BIN_SRC="target/release/${BIN_NAME}"
HOST_SRC="target/release/${HOST_NAME}"

if [ "$SKIP_BUILD" -eq 0 ]; then
  step "Building release binaries"
  npm --prefix apps/desktop run build
  ( cd apps/desktop && npx tauri build --no-bundle )
  cargo build --release -p zdm-host
fi

[ -f "$BIN_SRC" ]  || { echo "missing $BIN_SRC" >&2; exit 1; }
[ -f "$HOST_SRC" ] || { echo "missing $HOST_SRC" >&2; exit 1; }

step "Refreshing extension artefacts"
if python3 -c "import cryptography" 2>/dev/null; then
  python3 tools/build-packages.py >/dev/null
  ok "CRX3 + XPI rebuilt"
else
  warn "cryptography missing; reusing existing CRX/XPI if present"
fi

# --------------------------------------------------------------- shared payload
# One staging tree is built once and reused by all three package formats, which
# guarantees they contain byte-identical content.
stage_common() {
  local dest="$1"
  rm -rf "$dest"
  mkdir -p \
    "$dest/usr/bin" \
    "$dest/usr/lib/zdm" \
    "$dest/usr/share/zdm/extension" \
    "$dest/usr/share/applications" \
    "$dest/usr/share/icons/hicolor/scalable/apps" \
    "$dest/usr/share/doc/${PKG_NAME}"

  install -m 755 "$BIN_SRC"  "$dest/usr/bin/${BIN_NAME}"
  install -m 755 "$HOST_SRC" "$dest/usr/bin/${HOST_NAME}"

  cp -r apps/extension/. "$dest/usr/share/zdm/extension/"
  # Extension files must be world-readable: Chrome reads the manifest as the
  # installing user, and a 600 mode makes "Load unpacked" fail with
  # "Manifest is not valid JSON. Can't read file."
  chmod -R a+rX "$dest/usr/share/zdm/extension"
  [ -f assets/zdm.crx ] && install -m 644 assets/zdm.crx "$dest/usr/share/zdm/zdm.crx"
  [ -f "assets/${EXT_CHROME_ID}.crx" ] && \
    install -m 644 "assets/${EXT_CHROME_ID}.crx" "$dest/usr/share/zdm/${EXT_CHROME_ID}.crx"
  [ -f "assets/${EXT_GECKO_ID}.xpi" ] && \
    install -m 644 "assets/${EXT_GECKO_ID}.xpi" "$dest/usr/share/zdm/${EXT_GECKO_ID}.xpi"

  # XML update manifest for the Chromium policy: `update_url` must point at an
  # XML file, never at the CRX itself.
  if [ -f "assets/${EXT_CHROME_ID}.crx" ]; then
    write_update_manifest "$dest/usr/share/zdm/extension/update.xml" \
      "file:///usr/share/zdm/${EXT_CHROME_ID}.crx"
  fi

  install -m 644 assets/logo.svg "$dest/usr/share/zdm/logo.svg"

  for size in 16 32 48 64 128 256 512; do
    local src="assets/icons/${size}x${size}/apps/zdm.png"
    [ -f "$src" ] || src="assets/icon-${size}.png"
    [ -f "$src" ] || continue
    mkdir -p "$dest/usr/share/icons/hicolor/${size}x${size}/apps"
    install -m 644 "$src" "$dest/usr/share/icons/hicolor/${size}x${size}/apps/zdm.png"
  done
  install -m 644 assets/logo.svg "$dest/usr/share/icons/hicolor/scalable/apps/zdm.svg"

  install -m 644 packaging/linux/zdm.desktop "$dest/usr/share/applications/zdm.desktop"

  printf '%s\n' \
    "Zyzoom Download Manager ${VERSION}" \
    "Copyright (c) 2026 Ramy Badraan. All rights reserved." \
    "" \
    "${DESCRIPTION}" \
    "" \
    "Binaries: /usr/bin/${BIN_NAME}, /usr/bin/${HOST_NAME}" \
    "Data:     /usr/share/zdm" \
    "User data: ~/.local/share/zdm" \
    > "$dest/usr/share/doc/${PKG_NAME}/copyright"

  # chromedriver-style symlink is pointless here; keep the tree minimal.
  ln -sf "/usr/bin/${BIN_NAME}" "$dest/usr/lib/zdm/${BIN_NAME}"
}

# ---------------------------------------------------------------------- .deb
build_deb() {
  step "Building .deb"
  local stage="$STAGE/deb"
  stage_common "$stage"

  mkdir -p "$stage/DEBIAN"
  local installed_size
  installed_size="$(du -sk "$stage" | cut -f1)"

  cat > "$stage/DEBIAN/control" <<EOF
Package: ${PKG_NAME}
Version: ${VERSION}
Section: net
Priority: optional
Architecture: amd64
Installed-Size: ${installed_size}
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1
Recommends: yt-dlp, ffmpeg
Maintainer: ${MAINTAINER}
Homepage: ${HOMEPAGE}
Description: ${DESCRIPTION}
 Zyzoom Download Manager (ZDM) downloads files over multiple parallel
 connections, resumes interrupted transfers from the exact byte it stopped at,
 captures downloads from Chrome, Chromium, Brave, Edge, Vivaldi, Firefox and Zen,
 and downloads videos through yt-dlp.
 .
 Includes a systemd user service and XDG autostart entries, so it keeps running
 in the tray after login on every desktop, including Hyprland and sway.
EOF

  cat > "$stage/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
# Refresh caches so the menu entry and icons appear immediately.
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || true
fi

# Register the native messaging host for every user that has a browser profile
# AND for new users via /etc/skel-independent defaults. Per-user manifests are
# also written by the app itself on first run; this covers the immediate case.
install_host_manifest() {
  user="$1"
  home="$(getent passwd "$user" | cut -d: -f6)"
  [ -n "$home" ] && [ -d "$home" ] || return 0

  for dir in \
    "$home/.config/google-chrome" \
    "$home/.config/chromium" \
    "$home/.config/BraveSoftware/Brave-Browser" \
    "$home/.config/microsoft-edge" \
    "$home/.config/vivaldi"; do
    [ -d "$dir" ] || continue
    mkdir -p "$dir/NativeMessagingHosts" 2>/dev/null || continue
    cat > "$dir/NativeMessagingHosts/com.zyzoom.zdm.json" <<JSON
{
  "name": "com.zyzoom.zdm",
  "description": "Zyzoom Download Manager native host",
  "path": "/usr/bin/zdm-host",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://jgoccphfikadefpdemmaohkpbomgjcak/"]
}
JSON
    chown -R "$user" "$dir/NativeMessagingHosts" 2>/dev/null || true
  done

  mkdir -p "$home/.mozilla/native-messaging-hosts" 2>/dev/null || true
  cat > "$home/.mozilla/native-messaging-hosts/com.zyzoom.zdm.json" <<JSON
{
  "name": "com.zyzoom.zdm",
  "description": "Zyzoom Download Manager native host",
  "path": "/usr/bin/zdm-host",
  "type": "stdio",
  "allowed_extensions": ["zdm@zyzoom.com"]
}
JSON
  chown -R "$user" "$home/.mozilla/native-messaging-hosts" 2>/dev/null || true
}

# Only touch real human accounts with a home directory.
awk -F: '$3 >= 1000 && $3 < 60000 && $7 !~ /(nologin|false)$/ {print $1}' /etc/passwd |
while read -r u; do install_host_manifest "$u"; done

exit 0
POSTINST

  cat > "$stage/DEBIAN/prerm" <<'PRERM'
#!/bin/sh
set -e
# Stop the user service before the binary disappears.
if command -v systemctl >/dev/null 2>&1; then
  systemctl --user stop zdm.service >/dev/null 2>&1 || true
fi
pkill -x zdm >/dev/null 2>&1 || true
pkill -x zdm-host >/dev/null 2>&1 || true
exit 0
PRERM

  chmod 755 "$stage/DEBIAN/postinst" "$stage/DEBIAN/prerm"

  local target="$OUT/${PKG_NAME}_${VERSION}_amd64.deb"
  # --root-owner-group keeps file ownership root:root without needing fakeroot.
  if dpkg-deb --help >/dev/null 2>&1; then
    dpkg-deb --root-owner-group -Zxz --build "$stage" "$target" >/dev/null
  else
    # Portable fallback: build the ar archive ourselves (Debian format 2.0).
    # dpkg-deb normally writes md5sums; generate it here so `dpkg --verify`
    # works on packages built without dpkg installed.
    ( cd "$stage" && find . -path ./DEBIAN -prune -o -type f -printf '%P\0' \
        | LC_ALL=C sort -z | xargs -0 -r md5sum > "$stage/DEBIAN/md5sums" )
    chmod 644 "$stage/DEBIAN/md5sums"
    local tmp="$STAGE/deb-ar"
    rm -rf "$tmp"; mkdir -p "$tmp"
    ( cd "$stage" && find . ! -path "./DEBIAN" ! -path "./DEBIAN/*" -printf '%P\0' \
        | sort -z | tar --null --no-recursion -T - --owner=0 --group=0 --numeric-owner -cf "$tmp/data.tar" )
    ( cd "$stage/DEBIAN" && find . -type f -printf '%P\0' | sort -z \
        | tar --null -T - --owner=0 --group=0 --numeric-owner -cf "$tmp/control.tar" )
    echo "2.0" > "$tmp/debian-binary"
    xz -k -9 "$tmp/data.tar" "$tmp/control.tar"
    ( cd "$tmp" && ar rc "$target" debian-binary control.tar.xz data.tar.xz )
  fi

  ok "$(basename "$target") — $(du -h "$target" | cut -f1)"
}

# ---------------------------------------------------------------------- .rpm
build_rpm() {
  step "Building .rpm"
  local stage="$STAGE/rpm"
  stage_common "$stage"

  local target="$OUT/${PKG_NAME}-${VERSION}-1.x86_64.rpm"

  if command -v rpmbuild >/dev/null 2>&1; then
    # Fedora/RHEL: use the real toolchain so the signature and payload are
    # produced exactly as Fedora expects.
    local top="$STAGE/rpmbuild"
    rm -rf "$top"
    mkdir -p "$top"/{BUILD,RPMS,SOURCES,SPECS,SRPMS,BUILDROOT}
    tar -C "$stage/usr" -czf "$top/SOURCES/${PKG_NAME}-${VERSION}.tar.gz" .
    cp packaging/linux/zdm.spec "$top/SPECS/zdm.spec"
    if rpmbuild --define "_topdir $top" --define "_version ${VERSION}" \
                -bb "$top/SPECS/zdm.spec" >/dev/null 2>&1; then
      local built
      built="$(find "$top/RPMS" -name '*.rpm' -print -quit)"
      if [ -n "$built" ]; then
        cp "$built" "$OUT/"
        ok "$(basename "$built") — $(du -h "$built" | cut -f1)"
        return 0
      fi
    fi
    warn "rpmbuild failed; falling back to the direct writer"
  fi

  # Arch/Debian/containers: write the binary RPM ourselves.
  if python3 tools/mk-rpm.py "$stage" "$target" \
      --name "$PKG_NAME" \
      --version "$VERSION" \
      --summary "$DESCRIPTION" \
      --description "$DESCRIPTION" \
      --license "Proprietary" \
      --url "$HOMEPAGE" \
      --require "webkit2gtk4.1" \
      --require "gtk3" \
      --require "libayatana-appindicator-gtk3" \
      --post "$RPM_POST_SCRIPT" \
      --preun "$RPM_PREUN_SCRIPT" \
      --postun "$RPM_POSTUN_SCRIPT" >/dev/null; then
    ok "$(basename "$target") — $(du -h "$target" | cut -f1)"
  else
    warn "RPM creation failed; shipping the spec only"
    cp packaging/linux/zdm.spec "$OUT/zdm.spec"
  fi
}

# ------------------------------------------------------------------ AppImage
build_appimage() {
  step "Building AppImage"
  local appdir="$STAGE/AppDir"
  rm -rf "$appdir"
  mkdir -p "$appdir"/{usr/bin,usr/lib,usr/share/applications,usr/share/icons/hicolor}

  install -m 755 "$BIN_SRC"  "$appdir/usr/bin/${BIN_NAME}"
  install -m 755 "$HOST_SRC" "$appdir/usr/bin/${HOST_NAME}"
  mkdir -p "$appdir/usr/share/zdm/extension"
  cp -r apps/extension/. "$appdir/usr/share/zdm/extension/"
  chmod -R a+rX "$appdir/usr/share/zdm/extension"
  [ -f "assets/${EXT_CHROME_ID}.crx" ] && \
    cp "assets/${EXT_CHROME_ID}.crx" "$appdir/usr/share/zdm/"
  [ -f "assets/${EXT_GECKO_ID}.xpi" ] && \
    cp "assets/${EXT_GECKO_ID}.xpi" "$appdir/usr/share/zdm/"

  for size in 32 64 128 256 512; do
    local src="assets/icons/${size}x${size}/apps/zdm.png"
    [ -f "$src" ] || src="assets/icon-${size}.png"
    mkdir -p "$appdir/usr/share/icons/hicolor/${size}x${size}/apps"
    [ -f "$src" ] && cp "$src" "$appdir/usr/share/icons/hicolor/${size}x${size}/apps/zdm.png"
  done

  # Top-level entries required by the AppImage spec.
  cp assets/icon-512.png "$appdir/zdm.png" 2>/dev/null || cp assets/icon-256.png "$appdir/zdm.png"
  cp assets/logo.svg "$appdir/zdm.svg" 2>/dev/null || true

  cat > "$appdir/zdm.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Zyzoom Download Manager
Name[ar]=زي زوم مدير التنزيل
Name[fr]=Zyzoom Download Manager
Comment=${DESCRIPTION}
Exec=zdm %U
Icon=zdm
Terminal=false
Categories=Network;FileTransfer;
StartupWMClass=zdm
X-AppImage-Name=Zyzoom Download Manager
X-AppImage-Version=${VERSION}
EOF

  cat > "$appdir/AppRun" <<'APPRUN'
#!/bin/bash
# AppRun: resolve the bundle directory even through symlinks, then hand over.
HERE="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"
export PATH="${HERE}/usr/bin:${PATH}"
export LD_LIBRARY_PATH="${HERE}/usr/lib:${LD_LIBRARY_PATH:-}"
# The bundled host must be discoverable by the native-messaging manifests.
export ZDM_APPIMAGE_DIR="${HERE}"

# AppImages cannot write to their own mount, so first-run browser registration
# uses the per-user data directory instead (the app handles this itself).
exec "${HERE}/usr/bin/zdm" "$@"
APPRUN
  chmod 755 "$appdir/AppRun"

  local target="$OUT/Zyzoom_Download_Manager-${VERSION}-x86_64.AppImage"

  if command -v appimagetool >/dev/null 2>&1; then
    ARCH=x86_64 appimagetool --no-appstream "$appdir" "$target" >/dev/null
    ok "$(basename "$target") — $(du -h "$target" | cut -f1)"
    return 0
  fi

  # No appimagetool: assemble the type-2 image manually (runtime + squashfs).
  local runtime=""
  for candidate in /usr/lib/appimage/runtime-x86_64 "$ROOT/.build/runtime-x86_64"; do
    [ -f "$candidate" ] && runtime="$candidate" && break
  done

  if [ -z "$runtime" ]; then
    mkdir -p "$ROOT/.build"
    if curl -fsSL --max-time 90 \
        -o "$ROOT/.build/runtime-x86_64" \
        "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64"; then
      runtime="$ROOT/.build/runtime-x86_64"
    fi
  fi

  if [ -z "$runtime" ] || [ ! -f "$runtime" ]; then
    warn "AppImage runtime unavailable (offline?) — skipping AppImage"
    return 0
  fi

  local squash="$STAGE/zdm.squashfs"
  rm -f "$squash"
  mksquashfs "$appdir" "$squash" -noappend -comp zstd -b 1M -quiet

  cat "$runtime" "$squash" > "$target"
  chmod 755 "$target"
  ok "$(basename "$target") — $(du -h "$target" | cut -f1)"
}

# ----------------------------------------------------------------------- main
case "$ONLY" in
  deb)      build_deb ;;
  rpm)      build_rpm ;;
  appimage) build_appimage ;;
  "")
    build_deb
    build_rpm
    build_appimage
    ;;
  *) echo "Unknown --only value: $ONLY" >&2; exit 1 ;;
esac

printf '\n%s\n' "${GREEN}${BOLD}  ✓ Packages written to ${OUT}${NC}"
ls -lh "$OUT" 2>/dev/null | tail -n +2 | awk '{printf "    %-52s %s\n", $9, $5}'
printf '\n'

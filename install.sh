#!/usr/bin/env bash
# ==============================================================================
# Zyzoom Download Manager (ZDM) — Universal Linux Installer
# Developed by Ramy Badraan
#
# Installs the app, the native-messaging host, the browser extension, icons,
# desktop entries and — crucially — *all three* autostart mechanisms:
#   • XDG autostart      (GNOME, KDE, XFCE, Cinnamon)
#   • systemd --user     (works in Hyprland/sway/i3, which ignore XDG autostart)
#   • compositor exec-once (Hyprland, sway, i3) appended, never rewritten
#
# Usage:
#   ./install.sh              install for the current user
#   ./install.sh --system     install system-wide (needs sudo)
#   ./install.sh --rebuild    force a release rebuild first
#   ./install.sh --deps       install runtime dependencies only
#   ./install.sh --uninstall  remove everything this script created
#   ./install.sh --dry-run    print what would happen, change nothing
# ==============================================================================
set -euo pipefail

APP_NAME="Zyzoom Download Manager"
APP_SHORT="ZDM"
BIN_NAME="zdm"
HOST_NAME="zdm-host"
EXT_CHROME_ID="jgoccphfikadefpdemmaohkpbomgjcak"
EXT_GECKO_ID="zdm@zyzoom.com"

# Read from the manifest so the version can never drift from the shipped add-on.
# Chromium ignores an extension whose declared external_version is unchanged, so a
# hard-coded literal would pin users to whatever release shipped first.
EXT_VERSION="$(python3 -c "import json;print(json.load(open('apps/extension/manifest.json'))['version'])" 2>/dev/null || echo "0.0.0")"
NATIVE_HOST="com.zyzoom.zdm"

RED=$'\033[0;31m'; GREEN=$'\033[0;32m'; CYAN=$'\033[0;36m'
YELLOW=$'\033[1;33m'; BOLD=$'\033[1m'; DIM=$'\033[2m'; NC=$'\033[0m'

info()  { printf '%s\n' "${CYAN}▸${NC} $*"; }
ok()    { printf '%s\n' "  ${GREEN}✓${NC} $*"; }
warn()  { printf '%s\n' "  ${YELLOW}!${NC} $*"; }
fail()  { printf '%s\n' "${RED}✗ $*${NC}" >&2; exit 1; }
step()  { printf '\n%s\n' "${BOLD}${CYAN}$*${NC}"; }

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

# ------------------------------------------------------------------ arguments
MODE="user"
DO_DEPS=0
DO_REBUILD=0
DO_UNINSTALL=0
DRY_RUN=0

for arg in "$@"; do
  case "$arg" in
    --system)    MODE="system" ;;
    --user)      MODE="user" ;;
    --deps)      DO_DEPS=1 ;;
    --rebuild|-r) DO_REBUILD=1 ;;
    --uninstall|-u) DO_UNINSTALL=1 ;;
    --dry-run|-n) DRY_RUN=1 ;;
    --help|-h)
      sed -n '2,26p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    *) fail "Unknown option: $arg (try --help)" ;;
  esac
done

run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '   %s\n' "${DIM}[dry-run] $*${NC}"
  else
    "$@"
  fi
}

# Write stdin to a file, honouring --dry-run like `run` does.
run_write() {
  local target="$1"
  if [ "$DRY_RUN" -eq 1 ]; then
    printf '   %s\n' "${DIM}[dry-run] write ${target}${NC}"
    cat >/dev/null
  else
    mkdir -p "$(dirname "$target")"
    cat >"$target"
  fi
}

# ------------------------------------------------------------------- locations
if [ "$MODE" = "system" ]; then
  [ "$(id -u)" -eq 0 ] || fail "--system needs root: sudo ./install.sh --system"
  PREFIX="/usr"
  BIN_DIR="${PREFIX}/bin"
  LIB_DIR="${PREFIX}/lib/zdm"
  SHARE_DIR="${PREFIX}/share/zdm"
  APPS_DIR="${PREFIX}/share/applications"
  ICONS_DIR="${PREFIX}/share/icons/hicolor"
  AUTOSTART_DIR="/etc/xdg/autostart"
else
  PREFIX="${HOME}/.local"
  BIN_DIR="${PREFIX}/bin"
  LIB_DIR="${PREFIX}/lib/zdm"
  SHARE_DIR="${PREFIX}/share/zdm"
  APPS_DIR="${PREFIX}/share/applications"
  ICONS_DIR="${PREFIX}/share/icons/hicolor"
  AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
fi

CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/zdm"

USER_HOME="$HOME"
if [ "$MODE" = "system" ] && [ -n "${SUDO_USER:-}" ]; then
  USER_HOME="$(getent passwd "$SUDO_USER" | cut -d: -f6)"
fi

# ------------------------------------------------------------------ uninstall
if [ "$DO_UNINSTALL" -eq 1 ]; then
  step "Removing ${APP_NAME}"

  run pkill -x "$BIN_NAME" 2>/dev/null || true
  run pkill -x "$HOST_NAME" 2>/dev/null || true

  # systemd user unit
  if command -v systemctl >/dev/null 2>&1; then
    run systemctl --user disable --now zdm.service 2>/dev/null || true
  fi
  run rm -f "$CONFIG_DIR/systemd/user/zdm.service"
  run rm -f "$CONFIG_DIR/autostart/zdm.desktop"
  run rm -f "$APPS_DIR/zdm.desktop"

  run rm -f "$BIN_DIR/$BIN_NAME" "$BIN_DIR/$HOST_NAME"
  run rm -rf "$LIB_DIR" "$SHARE_DIR"

  for size in 16 32 48 64 128 256 512; do
    run rm -f "$ICONS_DIR/${size}x${size}/apps/zdm.png"
  done
  run rm -f "$ICONS_DIR/scalable/apps/zdm.svg"

  # Native messaging manifests
  for browser in google-chrome chromium BraveSoftware/Brave-Browser microsoft-edge vivaldi; do
    run rm -f "$USER_HOME/.config/$browser/NativeMessagingHosts/$NATIVE_HOST.json"
  done
  run rm -f "$USER_HOME/.mozilla/native-messaging-hosts/$NATIVE_HOST.json"
  run rm -f "$CONFIG_DIR/zen/native-messaging-hosts/$NATIVE_HOST.json"

  # Compositor lines we appended. Marker-based, never a blanket rewrite, and the
  # Lua hook needs the same treatment as the classic conf files.
  for cfg in "$USER_HOME/.config/hypr/hyprland.conf" "$USER_HOME/.config/hypr/exec.conf" \
             "$USER_HOME/.config/sway/config" "$USER_HOME/.config/i3/config"; do
    if [ -f "$cfg" ] && grep -q "# ZDM autostart" "$cfg" 2>/dev/null; then
      run sed -i '/# ZDM autostart/d; /ZDM autostart (managed/d' "$cfg"
      run sed -i "\|$BIN_DIR/$BIN_NAME --background|d" "$cfg"
    fi
  done

  # Hyprland Lua hook: drop our marker and our exec_cmd line only.
  for lua in "$USER_HOME/.config/hypr/confs/startup.lua" "$USER_HOME"/.config/hypr/confs/*.lua; do
    [ -f "$lua" ] || continue
    grep -q "ZDM autostart" "$lua" 2>/dev/null || continue
    if [ "$DRY_RUN" -eq 0 ]; then
      python3 - "$lua" <<'PYEOF'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
kept = [
    line for line in p.read_text().splitlines()
    if "ZDM autostart (managed" not in line
    and not ("hl.exec_cmd" in line and "zdm" in line and "--background" in line)
]
p.write_text("\n".join(kept) + "\n")
PYEOF
    fi
  done

  warn "Your downloads and settings in $DATA_DIR were kept."
  warn "Delete that folder manually if you want a clean slate."
  ok "Uninstalled."
  exit 0
fi

# --------------------------------------------------------------- dependencies
install_deps() {
  step "Runtime dependencies"

  local sudo_cmd=""
  [ "$(id -u)" -ne 0 ] && command -v sudo >/dev/null 2>&1 && sudo_cmd="sudo"

  if command -v pacman >/dev/null 2>&1; then
    info "Arch family detected"
    run $sudo_cmd pacman -S --needed --noconfirm \
      webkit2gtk-4.1 libayatana-appindicator gtk3 librsvg \
      yt-dlp ffmpeg python-cryptography 2>/dev/null || warn "Some packages may be unavailable"
  elif command -v dnf >/dev/null 2>&1; then
    info "Fedora family detected"
    run $sudo_cmd dnf install -y \
      webkit2gtk4.1 libayatana-appindicator-gtk3 gtk3 librsvg2 \
      yt-dlp ffmpeg-free python3-cryptography 2>/dev/null || \
    run $sudo_cmd dnf install -y webkit2gtk4.1 libayatana-appindicator-gtk3 gtk3 yt-dlp 2>/dev/null || \
      warn "Install webkit2gtk4.1 and libayatana-appindicator-gtk3 manually"
  elif command -v apt-get >/dev/null 2>&1; then
    info "Debian family detected"
    run $sudo_cmd apt-get update -qq
    run $sudo_cmd apt-get install -y \
      libwebkit2gtk-4.1-0 libayatana-appindicator3-1 libgtk-3-0 librsvg2-bin \
      yt-dlp ffmpeg python3-cryptography 2>/dev/null || \
      warn "Install libwebkit2gtk-4.1-0 and libayatana-appindicator3-1 manually"
  elif command -v zypper >/dev/null 2>&1; then
    info "openSUSE detected"
    run $sudo_cmd zypper --non-interactive install \
      libwebkit2gtk-4_1-0 libayatana-appindicator3-1 gtk3 rsvg-convert \
      yt-dlp ffmpeg python3-cryptography 2>/dev/null || warn "Some packages may be unavailable"
  else
    warn "Unknown package manager — install webkit2gtk 4.1, GTK 3 and AppIndicator manually."
  fi

  # ffmpeg merges the separate video and audio streams. Without it every
  # download above 720p lands as two files (title.f137.mp4 + title.f140.m4a).
  if ! command -v ffmpeg >/dev/null 2>&1; then
    warn "ffmpeg is missing: video downloads would produce separate audio/video files."
    warn "Install it with your package manager (it is in every distro's repos)."
  else
    ok "ffmpeg present ($(ffmpeg -version 2>/dev/null | head -1 | cut -d' ' -f1-3))"
  fi

  # yt-dlp standalone fallback (the binary works even with an old Python).
  if ! command -v yt-dlp >/dev/null 2>&1; then
    info "Fetching the standalone yt-dlp binary"
    run mkdir -p "${DATA_DIR}/bin"
    if [ "$DRY_RUN" -eq 0 ]; then
      curl -fsSL https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp \
        -o "${DATA_DIR}/bin/yt-dlp" && chmod +x "${DATA_DIR}/bin/yt-dlp" \
        || warn "yt-dlp download failed — the app can retry it from Settings."
    fi
  fi
}

[ "$DO_DEPS" -eq 1 ] && { install_deps; step "Done (dependencies only)"; exit 0; }

# --------------------------------------------------------------------- banner
printf '\n%s\n' "${BOLD}${CYAN}  ⚡ ${APP_NAME} — Linux installer${NC}"
printf '%s\n' "  ${DIM}Developed by Ramy Badraan${NC}"
printf '%s\n' "  ${DIM}mode: ${MODE}$([ "$DRY_RUN" -eq 1 ] && echo ' · dry-run')${NC}"

install_deps

# ------------------------------------------------------------------ build/step
step "Preparing binaries"

command -v cargo >/dev/null 2>&1 || fail "Rust is required to build: https://rustup.rs"
command -v node  >/dev/null 2>&1 || fail "Node.js is required to build: https://nodejs.org"

# rustup stores its toolchains under $RUSTUP_HOME, which defaults to $HOME/.rustup.
# If HOME is redirected (a test harness, sudo, a different user) rustup finds no
# default toolchain and `tauri build` dies with a confusing cargo-metadata error.
if ! cargo metadata --no-deps --format-version 1 >/dev/null 2>&1; then
  if [ -z "${RUSTUP_HOME:-}" ] && [ -d "${SUDO_USER:+/home/$SUDO_USER/.rustup}" ]; then
    export RUSTUP_HOME="/home/$SUDO_USER/.rustup"
    export CARGO_HOME="/home/$SUDO_USER/.cargo"
  fi
fi
cargo metadata --no-deps --format-version 1 >/dev/null 2>&1 \
  || fail "cargo cannot read the workspace. If rustup reports 'no default toolchain', run: rustup default stable"

if [ ! -d node_modules ] && [ -d apps/desktop/node_modules ]; then
  :
elif [ ! -d apps/desktop/node_modules ]; then
  info "Installing frontend dependencies"
  run npm --prefix apps/desktop install --no-audit --no-fund
fi

# Extension artefacts (CRX3 for Chromium, XPI for Firefox).
#
# The CRX must be signed with our key; python-cryptography does that. Without it
# the extension cannot auto-install and the user has to load it by hand, so the
# problem is reported *now* with the exact command, not as a late warning.
info "Packaging the browser extension"

if [ "$DRY_RUN" -eq 0 ]; then
  if python3 -c "import cryptography" 2>/dev/null; then
    if python3 tools/build-packages.py >/dev/null 2>&1; then
      ok "Signed CRX3 and Firefox XPI rebuilt"
    else
      warn "CRX signing failed (is packaging/keys/zdm-extension.pem present?)"
      warn "Re-sign it with: python3 tools/build-packages.py"
    fi
  else
    warn "python-cryptography is missing, so the extension cannot be signed."
    warn "Install it and re-run this script:"
    warn "  Arch:    sudo pacman -S python-cryptography"
    warn "  Debian:  sudo apt install python3-cryptography"
    warn "  Fedora:  sudo dnf install python3-cryptography"
    if [ -f "assets/${EXT_CHROME_ID}.crx" ]; then
      warn "Using the CRX already in assets/ (it may be stale)."
    fi
  fi
fi

BIN_SRC="target/release/${BIN_NAME}"
HOST_SRC="target/release/${HOST_NAME}"

needs_build=0
[ "$DO_REBUILD" -eq 1 ] && needs_build=1
[ ! -f "$BIN_SRC" ] && needs_build=1
[ ! -f "$HOST_SRC" ] && needs_build=1
# Rebuild when anything that gets compiled in changed: components, styles, the
# bundled fonts and icons (public/), the extension payload (assets/), or the
# Tauri config. Missing public/ here is why a font change could ship stale.
if [ -f "$BIN_SRC" ]; then
  for watched in \
    apps/desktop/src \
    apps/desktop/public \
    apps/desktop/index.html \
    apps/desktop/src-tauri/src \
    apps/desktop/src-tauri/tauri.conf.json \
    apps/desktop/src-tauri/Cargo.toml \
    apps/extension \
    assets; do
    [ -e "$watched" ] || continue
    if [ -n "$(find "$watched" -newer "$BIN_SRC" -print -quit 2>/dev/null)" ]; then
      needs_build=1
      break
    fi
  done
fi

if [ "$needs_build" -eq 1 ]; then
  info "Building the release binary (this takes a few minutes the first time)"
  run npm --prefix apps/desktop run build
  # `tauri build` is what embeds the frontend (it enables the custom-protocol
  # feature). A bare `cargo build` would produce a binary that tries to reach
  # http://localhost:1420 and shows "Connection refused".
  run bash -c "cd apps/desktop && npx tauri build --no-bundle"
  run cargo build --release -p zdm-host
else
  ok "Existing release build is up to date"
fi

if [ "$DRY_RUN" -eq 0 ]; then
  [ -f "$BIN_SRC" ] || fail "Build did not produce $BIN_SRC"

  # Guard against the "Connection refused" regression: a release binary must
  # serve its UI from inside itself.
  # `grep -q` closes the pipe early; under `set -o pipefail` that turns `strings`
  # into SIGPIPE and the whole check reports a false failure. Capture first.
  EMBEDDED_ASSET="$(strings "$BIN_SRC" 2>/dev/null | grep -oE "index-[A-Za-z0-9_-]+\.js" | head -1 || true)"
  if [ -n "$EMBEDDED_ASSET" ]; then
    ok "Frontend is embedded in the binary ($EMBEDDED_ASSET)"
  else
    fail "The binary has no embedded frontend; it would open a blank window.
       Rebuild with: cd apps/desktop && npx tauri build --no-bundle"
  fi

  # The extension payload is compiled in, so the CRX must exist at build time —
  # and it must not be older than the extension sources, or the app ships a
  # binary whose embedded add-on predates the code in apps/extension.
  # Cairo is the requested Arabic face. If the subsets are missing the UI falls
  # back to IBM Plex silently, which looks like "the font change did nothing".
  if ls apps/desktop/public/fonts/Cairo-*.woff2 >/dev/null 2>&1; then
    ok "Cairo Arabic font present"
  else
    warn "Cairo font files are missing; Arabic will fall back to IBM Plex."
    warn "Restore them with: tools/fetch-fonts.sh"
  fi

  CRX_PATH="assets/${EXT_CHROME_ID}.crx"
  if [ ! -f "$CRX_PATH" ]; then
    warn "No signed CRX. Run: python3 tools/build-packages.py, then re-run this script."
  elif [ -n "$(find apps/extension -newer "$CRX_PATH" -print -quit 2>/dev/null)" ]; then
    warn "The CRX is older than apps/extension; the embedded add-on would be stale."
    warn "Re-sign it and rebuild: python3 tools/build-packages.py && ./install.sh --rebuild"
  else
    ok "Signed CRX present and current (bundled into the binary)"
  fi
fi

# ------------------------------------------------------------------ file copy
step "Installing files"
run mkdir -p "$BIN_DIR" "$LIB_DIR" "$SHARE_DIR" "$APPS_DIR"

# Replacing the binary under a running process leaves the old image mapped, and
# any floating window it owns keeps serving the previous frontend — which is
# exactly how a stale prompt window survives an upgrade. Wait for it to exit.
if [ "$DRY_RUN" -eq 0 ]; then
  pkill -x "$BIN_NAME" 2>/dev/null || true
  pkill -x "$HOST_NAME" 2>/dev/null || true

  waited=0
  while pgrep -x "$BIN_NAME" >/dev/null 2>&1 && [ "$waited" -lt 40 ]; do
    sleep 0.25
    waited=$((waited + 1))
  done

  if pgrep -x "$BIN_NAME" >/dev/null 2>&1; then
    warn "${APP_NAME} is still running and could not be stopped."
    warn "Close every window (including the small download prompt), then re-run:"
    warn "  ${BIN_DIR}/${BIN_NAME} --uninstall && ./install.sh"
    exit 1
  fi
fi

run install -m 755 "$BIN_SRC" "$BIN_DIR/$BIN_NAME"
run install -m 755 "$HOST_SRC" "$BIN_DIR/$HOST_NAME"

# Extension payload next to the binaries so `find_extension_dir` finds it.
run rm -rf "${SHARE_DIR}/extension"
run cp -r apps/extension "${SHARE_DIR}/extension"
run chmod -R a+rX "${SHARE_DIR}/extension"
[ -f assets/zdm.crx ] && run cp assets/zdm.crx "${SHARE_DIR}/zdm.crx"
[ -f "assets/${EXT_CHROME_ID}.crx" ] && run cp "assets/${EXT_CHROME_ID}.crx" "${SHARE_DIR}/${EXT_CHROME_ID}.crx"
[ -f "assets/${EXT_GECKO_ID}.xpi" ] && run cp "assets/${EXT_GECKO_ID}.xpi" "${SHARE_DIR}/${EXT_GECKO_ID}.xpi"

# XML update manifest for the Chromium policy.
#
# `update_url` must point at this file, never at the .crx: Chromium parses the
# manifest as XML and drops the whole policy when it finds a ZIP. The app also
# writes one into its data dir, this covers the system-wide install.
if [ -f "assets/${EXT_CHROME_ID}.crx" ]; then
  ext_version=$(python3 -c "import json;print(json.load(open('apps/extension/manifest.json'))['version'])" 2>/dev/null || echo "1.0.0")
  run_write "${SHARE_DIR}/extension/update.xml" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<gupdate xmlns="http://www.google.com/update2/response" protocol="2.0">
  <app appid="${EXT_CHROME_ID}">
    <updatecheck codebase="file://${SHARE_DIR}/${EXT_CHROME_ID}.crx" version="${ext_version}" />
  </app>
</gupdate>
XML
fi

run cp assets/logo.svg "${SHARE_DIR}/logo.svg"

# Icons
info "Installing icons"
for size in 16 32 48 64 128 256 512; do
  src="assets/icons/${size}x${size}/apps/zdm.png"
  [ -f "$src" ] || src="assets/icon-${size}.png"
  if [ -f "$src" ]; then
    run mkdir -p "$ICONS_DIR/${size}x${size}/apps"
    run install -m 644 "$src" "$ICONS_DIR/${size}x${size}/apps/zdm.png"
  fi
done
run mkdir -p "$ICONS_DIR/scalable/apps"
run install -m 644 assets/logo.svg "$ICONS_DIR/scalable/apps/zdm.svg"
command -v gtk-update-icon-cache >/dev/null 2>&1 && \
  run gtk-update-icon-cache -f -t "$ICONS_DIR" 2>/dev/null || true

# --------------------------------------------------------------- desktop entry
step "Desktop integration"
DESKTOP_FILE="${APPS_DIR}/zdm.desktop"
if [ "$DRY_RUN" -eq 0 ]; then
  cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Type=Application
Version=1.1
Name=${APP_NAME}
Name[ar]=زي زوم مدير التنزيل
Name[fr]=Zyzoom Download Manager
GenericName=Download Manager
GenericName[ar]=مدير التنزيل
GenericName[fr]=Gestionnaire de téléchargement
Comment=Multi-segment download accelerator with browser capture and video download
Comment[ar]=مُسرّع تنزيل متعدد الاتصالات مع التقاط من المتصفح وتنزيل الفيديو
Comment[fr]=Accélérateur de téléchargement multi-segments avec capture navigateur
Exec=${BIN_DIR}/${BIN_NAME} %U
TryExec=${BIN_DIR}/${BIN_NAME}
Icon=zdm
Terminal=false
Categories=Network;FileTransfer;Qt;GTK;
Keywords=download;manager;accelerator;video;
Keywords[ar]=تنزيل;مدير;فيديو;تحميل
Keywords[fr]=téléchargement;gestionnaire;vidéo;
StartupNotify=true
StartupWMClass=zdm
SingleMainWindow=true
Actions=AddUrl;Video;Torrent;
MimeType=application/x-bittorrent;x-scheme-handler/magnet;

[Desktop Action AddUrl]
Name=Add download link
Name[ar]=إضافة رابط تنزيل
Name[fr]=Ajouter un lien
Exec=${BIN_DIR}/${BIN_NAME} --add-url

[Desktop Action Video]
Name=Download a video
Name[ar]=تنزيل فيديو
Name[fr]=Télécharger une vidéo
Exec=${BIN_DIR}/${BIN_NAME} --video

[Desktop Action Torrent]
Name=Open a torrent file
Name[ar]=فتح ملف تورنت
Name[fr]=Ouvrir un fichier torrent
Exec=${BIN_DIR}/${BIN_NAME} --add-torrent
EOF
  chmod 644 "$DESKTOP_FILE"
fi
ok "Application menu entry created"

# Torrent file association (Linux).
#
# `xdg-mime default` needs the .desktop file id, and the MIME type must be
# declared on the entry itself (done above). Without this a double-clicked
# .torrent opens the archive viewer, never ZDM.
#
# `magnet:` is a URI scheme, not an extension: clicking one in a browser is
# handled by the desktop's scheme registry, so the entry must also advertise
# `x-scheme-handler/magnet`. Without it a magnet link opened nothing at all,
# which looked exactly like "torrents are unsupported".
if [ "$DRY_RUN" -eq 0 ] && command -v xdg-mime >/dev/null 2>&1; then
  xdg-mime default zdm.desktop application/x-bittorrent 2>/dev/null \
    && ok "Torrent files open in ZDM" \
    || warn "could not register the .torrent association (xdg-mime missing)"

  xdg-mime default zdm.desktop x-scheme-handler/magnet 2>/dev/null \
    && ok "Magnet links open in ZDM" \
    || warn "could not register the magnet: scheme (xdg-mime missing)"
fi

command -v desktop-file-validate >/dev/null 2>&1 && \
  desktop-file-validate "$DESKTOP_FILE" 2>/dev/null && ok "desktop entry validates" || true
command -v update-desktop-database >/dev/null 2>&1 && \
  run update-desktop-database "$APPS_DIR" 2>/dev/null || true

# ------------------------------------------------------------------- autostart
step "Autostart (logind → tray)"
run mkdir -p "$AUTOSTART_DIR"
if [ "$DRY_RUN" -eq 0 ]; then
  cat > "${AUTOSTART_DIR}/zdm.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=${APP_NAME} (background)
Name[ar]=زي زوم مدير التنزيل (في الخلفية)
Name[fr]=Zyzoom Download Manager (arrière-plan)
Comment=Start ZDM minimized to the tray on login
Exec=${BIN_DIR}/${BIN_NAME} --background
Icon=zdm
Terminal=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=3
X-KDE-autostart-after=panel
StartupNotify=false
EOF
  chmod 644 "${AUTOSTART_DIR}/zdm.desktop"
fi
ok "XDG autostart entry created"

# systemd --user: the only mechanism that works in a bare Hyprland session.
SYSTEMD_DIR="${CONFIG_DIR}/systemd/user"
run mkdir -p "$SYSTEMD_DIR"
if [ "$DRY_RUN" -eq 0 ]; then
  cat > "${SYSTEMD_DIR}/zdm.service" <<EOF
[Unit]
Description=Zyzoom Download Manager (background service)
Documentation=file://${SHARE_DIR}
After=graphical-session.target network-online.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=${BIN_DIR}/${BIN_NAME} --background
Restart=on-failure
RestartSec=5
Nice=5
# Do not let a download burst starve the desktop.
IOSchedulingClass=best-effort
IOSchedulingPriority=6

[Install]
WantedBy=graphical-session.target
EOF
  chmod 644 "${SYSTEMD_DIR}/zdm.service"

  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user daemon-reload 2>/dev/null || true
    systemctl --user enable zdm.service 2>/dev/null \
      && ok "systemd user service enabled" \
      || warn "Could not enable the systemd unit (no user bus?) — it will start on next login."
  fi
fi

# Compositor hooks: Hyprland ignores XDG autostart entirely.
#
# Hyprland ships two configuration styles and they need different syntax:
#   * classic  -> `exec-once = "<cmd>"`
#   * modern   -> a split Lua tree (confs/startup.lua) using
#                 `hl.on("hyprland.start", function() hl.exec_cmd("<cmd>") end)`
# Writing only the classic form is a silent no-op on a modern setup, which is
# why the app never started with the session on Arch.
COMPOSITOR_WIRED=0
HYPR_DIR="$USER_HOME/.config/hypr"
ZDM_CMD="'$BIN_DIR/$BIN_NAME' --background"
LUA_MARKER="-- ZDM autostart (managed by Zyzoom Download Manager)"

wire_hypr_lua() {
  local file="$1"
  [ -f "$file" ] || return 1
  grep -q "$LUA_MARKER" "$file" 2>/dev/null && return 0

  if [ "$DRY_RUN" -eq 1 ]; then
    printf '   %s\n' "${DIM}[dry-run] insert hl.exec_cmd into $file${NC}"
    return 0
  fi

  # Insert inside the existing hyprland.start handler when there is one, so the
  # call runs in the right lifecycle phase and the file stays valid Lua.
  python3 - "$file" "$ZDM_CMD" "$LUA_MARKER" <<'PYEOF'
import sys, pathlib

path, command, marker = sys.argv[1], sys.argv[2], sys.argv[3]
p = pathlib.Path(path)
text = p.read_text()
if marker in text:
    sys.exit(0)

hook = text.find("hyprland.start")
if hook >= 0:
    close = text.find("end)", hook)
    if close >= 0:
        call = f'{marker}\n    hl.exec_cmd("{command}")\n'
        p.write_text(text[:close] + call + text[close:])
        sys.exit(0)

# No handler found: append a self-contained block.
p.write_text(text + f'\n{marker}\nhl.on("hyprland.start", function()\n    hl.exec_cmd("{command}")\nend)\n')
PYEOF
  return 0
}

# Modern Hyprland: prefer the dedicated startup.lua.
if [ -f "$HYPR_DIR/confs/startup.lua" ]; then
  if wire_hypr_lua "$HYPR_DIR/confs/startup.lua"; then
    COMPOSITOR_WIRED=1
    ok "Hyprland Lua startup hook written (confs/startup.lua)"
  fi
else
  # Fall back to any Lua file that already defines the start hook.
  for f in "$HYPR_DIR"/confs/*.lua; do
    [ -f "$f" ] || continue
    grep -q "hyprland.start" "$f" 2>/dev/null || continue
    if wire_hypr_lua "$f"; then
      COMPOSITOR_WIRED=1
      ok "Hyprland Lua startup hook written ($(basename "$f"))"
    fi
    break
  done
fi

# Classic Hyprland (and nothing else claims the hook).
if [ "$COMPOSITOR_WIRED" -eq 0 ]; then
  for cfg in "$HYPR_DIR/hyprland.conf" "$HYPR_DIR/exec.conf"; do
    if [ -f "$cfg" ] && ! grep -q "# ZDM autostart" "$cfg"; then
      if [ "$DRY_RUN" -eq 0 ]; then
        printf '\n# ZDM autostart (managed by Zyzoom Download Manager)\nexec-once = %s\n' \
          "$ZDM_CMD" >> "$cfg"
      fi
      COMPOSITOR_WIRED=1
      ok "Hyprland exec-once added to $(basename "$cfg")"
      break
    fi
  done
fi

if [ -f "$USER_HOME/.config/sway/config" ] && ! grep -q "# ZDM autostart" "$USER_HOME/.config/sway/config"; then
  if [ "$DRY_RUN" -eq 0 ]; then
    printf '\n# ZDM autostart (managed by Zyzoom Download Manager)\nexec %s --background\n' \
      "$BIN_DIR/$BIN_NAME" >> "$USER_HOME/.config/sway/config"
  fi
  COMPOSITOR_WIRED=1
  ok "sway exec added"
fi

if [ -f "$USER_HOME/.config/i3/config" ] && ! grep -q "# ZDM autostart" "$USER_HOME/.config/i3/config"; then
  if [ "$DRY_RUN" -eq 0 ]; then
    printf '\n# ZDM autostart (managed by Zyzoom Download Manager)\nexec --no-startup-id %s --background\n' \
      "$BIN_DIR/$BIN_NAME" >> "$USER_HOME/.config/i3/config"
  fi
  COMPOSITOR_WIRED=1
  ok "i3 exec added"
fi

# Tray host guidance: without one, no application can show an icon.
if [ "$COMPOSITOR_WIRED" -eq 1 ]; then
  if ! pgrep -x waybar >/dev/null 2>&1; then
    warn "Hyprland/sway detected. For a tray icon, add the \"tray\" module to waybar:"
    warn '  "modules-right": ["tray", ...]   (waybar ships a tray module by default)'
  fi
fi

# ---------------------------------------------------------- browser integration
step "Browser integration"

write_chromium_host() {
  local browser_dir="$1"
  [ -d "$browser_dir" ] || return 0
  mkdir -p "$browser_dir/NativeMessagingHosts"
  cat > "$browser_dir/NativeMessagingHosts/${NATIVE_HOST}.json" <<EOF
{
  "name": "${NATIVE_HOST}",
  "description": "Zyzoom Download Manager native host",
  "path": "${BIN_DIR}/${HOST_NAME}",
  "type": "stdio",
  "allowed_origins": ["chrome-extension://${EXT_CHROME_ID}/"]
}
EOF
  # Supported auto-install channel: signed CRX + external_version.
  # Policy: allows a file:// CRX and force-installs it, which is what stops the
  # browser asking whether to trust an add-on that is not from the store.
  if [ -f "assets/${EXT_CHROME_ID}.crx" ]; then
    mkdir -p "$browser_dir/policies"
    cat > "$browser_dir/policies/policies.json" <<POLICY
{
  "ExtensionInstallSources": ["file:///*"],
  "ExtensionSettings": {
    "${EXT_CHROME_ID}": {
      "installation_mode": "force_installed",
      "update_url": "file://${SHARE_DIR}/${EXT_CHROME_ID}.crx",
      "toolbar_pin": "force_pinned"
    }
  }
}
POLICY

    mkdir -p "$browser_dir/External Extensions"
    # external_version must equal the CRX's real version, or Chromium treats the
    # extension as already installed and never refreshes it — the previous
    # hard-coded value silently pinned every user to the first release.
    cat > "$browser_dir/External Extensions/${EXT_CHROME_ID}.json" <<EOF
{
  "external_crx": "${SHARE_DIR}/${EXT_CHROME_ID}.crx",
  "external_version": "${EXT_VERSION}"
}
EOF
  fi
  ok "$(basename "$browser_dir")"
}

write_gecko_host() {
  local base_dir="$1"
  mkdir -p "$base_dir"
  cat > "$base_dir/${NATIVE_HOST}.json" <<EOF
{
  "name": "${NATIVE_HOST}",
  "description": "Zyzoom Download Manager native host",
  "path": "${BIN_DIR}/${HOST_NAME}",
  "type": "stdio",
  "allowed_extensions": ["${EXT_GECKO_ID}"]
}
EOF
  ok "$(basename "$(dirname "$base_dir")")"
}

if [ "$DRY_RUN" -eq 0 ]; then
  for browser in google-chrome chromium BraveSoftware/Brave-Browser microsoft-edge vivaldi; do
    write_chromium_host "$USER_HOME/.config/$browser"
  done
  write_gecko_host "$USER_HOME/.mozilla/native-messaging-hosts"
  write_gecko_host "$CONFIG_DIR/zen/native-messaging-hosts"
fi

# Firefox/Zen: enterprise policy is the only route for a forced install.
if [ -f "assets/${EXT_GECKO_ID}.xpi" ] && [ "$DRY_RUN" -eq 0 ]; then
  for policy_dir in "$CONFIG_DIR/zen/distribution" "$USER_HOME/.mozilla/firefox/distribution"; do
    mkdir -p "$policy_dir"
    cat > "$policy_dir/policies.json" <<EOF
{
  "policies": {
    "ExtensionSettings": {
      "${EXT_GECKO_ID}": {
        "installation_mode": "force_installed",
        "install_url": "file://${SHARE_DIR}/${EXT_GECKO_ID}.xpi"
      }
    }
  }
}
EOF
  done
  ok "Firefox/Zen policy written (see notes below)"
fi

# ---------------------------------------------------------------------- finish
printf '\n%s\n' "${GREEN}${BOLD}  ✓ ${APP_NAME} installed${NC}"
printf '%s\n' "  ${DIM}binary     ${BIN_DIR}/${BIN_NAME}${NC}"
printf '%s\n' "  ${DIM}data       ${DATA_DIR}${NC}"
printf '%s\n' "  ${DIM}autostart  xdg$( [ -f "${SYSTEMD_DIR}/zdm.service" ] && printf ' + systemd' )$( [ "$COMPOSITOR_WIRED" -eq 1 ] && printf ' + compositor' )${NC}"

printf '\n%s\n' "${BOLD}Notes${NC}"
printf '%s\n' "  • Launch it once from the app menu; the tray icon appears if your bar has a tray host."
printf '%s\n' "  • Hyprland/sway: waybar must include \"tray\" in its modules for any icon to show."
printf '%s\n' "  • GNOME: install the \"AppIndicator and KStatusNotifierItem Support\" extension."
printf '%s\n' "  • Firefox (non-ESR) only installs Mozilla-signed add-ons. If the policy is ignored,"
printf '%s\n' "    open ${SHARE_DIR}/${EXT_GECKO_ID}.xpi once via about:addons."
printf '%s\n' "  • Chromium browsers pick the extension up on next start (external CRX)."
printf '\n%s\n\n' "  ${CYAN}${BOLD}Ramy Badraan${NC} ${DIM}· built for speed${NC}"

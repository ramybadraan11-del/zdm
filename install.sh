#!/usr/bin/env bash
# ==============================================================================
# Zyzoom Download Manager (ZDM) - Universal Linux Installer
# Developed by Ramy Badraan
#
# Supported Distros: Arch Linux / Manjaro / EndeavourOS,
#                    Fedora / RHEL / Alma / Rocky,
#                    Debian / Ubuntu / Linux Mint / Pop!_OS,
#                    openSUSE, and any standard Linux desktop.
# ==============================================================================

set -e

# Styling & Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m' # No Color

echo -e "${CYAN}${BOLD}"
cat << "EOF"
  ███████╗██████╗ ███╗   ███╗
  ╚══███╔╝██╔══██╗████╗ ████║
    ███╔╝ ██║  ██║██╔████╔██║
   ███╔╝  ██║  ██║██║╚██╔╝██║
  ███████╗██████╔╝██║ ╚═╝ ██║
  ╚══════╝╚═════╝ ╚═╝     ╚═╝
EOF
echo -e "${GREEN}Zyzoom Download Manager (ZDM) — Universal Linux Installer${NC}"
echo -e "${BLUE}Developed by ${CYAN}${BOLD}Ramy Badraan${NC}"
echo -e "${CYAN}------------------------------------------------------------${NC}"

# Detect if running as root or normal user
if [ "$EUID" -eq 0 ]; then
    IS_ROOT=1
    BIN_DIR="/usr/local/bin"
    SHARE_DIR="/usr/local/share/zdm"
    APPS_DIR="/usr/share/applications"
    ICONS_DIR="/usr/share/icons/hicolor"
    AUTOSTART_DIR="/etc/xdg/autostart"
    REAL_USER="${SUDO_USER:-$USER}"
    REAL_HOME=$(getent passwd "$REAL_USER" | cut -d: -f6)
else
    IS_ROOT=0
    BIN_DIR="$HOME/.local/bin"
    SHARE_DIR="$HOME/.local/share/zdm"
    APPS_DIR="$HOME/.local/share/applications"
    ICONS_DIR="$HOME/.local/share/icons/hicolor"
    AUTOSTART_DIR="$HOME/.config/autostart"
    REAL_USER="$USER"
    REAL_HOME="$HOME"
fi

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Function: Uninstall
if [ "$1" == "--uninstall" ] || [ "$1" == "-u" ]; then
    echo -e "${YELLOW}>> جارٍ إزالة تثبيت Zyzoom Download Manager (ZDM)...${NC}"
    rm -f "${BIN_DIR}/zdm"
    rm -f "${BIN_DIR}/zdm-host"
    rm -rf "${SHARE_DIR}"
    rm -f "${APPS_DIR}/zdm.desktop"
    rm -f "${AUTOSTART_DIR}/zdm.desktop"
    rm -f "${REAL_HOME}/.config/autostart/zdm.desktop"
    rm -f "${ICONS_DIR}/512x512/apps/zdm.png"
    rm -f "${ICONS_DIR}/256x256/apps/zdm.png"
    rm -f "${ICONS_DIR}/128x128/apps/zdm.png"
    rm -f "${ICONS_DIR}/32x32/apps/zdm.png"
    rm -f "${ICONS_DIR}/scalable/apps/zdm.svg"

    # Remove Native Messaging Hosts
    rm -f "${REAL_HOME}/.config/google-chrome/NativeMessagingHosts/com.zyzoom.zdm.json"
    rm -f "${REAL_HOME}/.config/chromium/NativeMessagingHosts/com.zyzoom.zdm.json"
    rm -f "${REAL_HOME}/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.zyzoom.zdm.json"
    rm -f "${REAL_HOME}/.config/microsoft-edge/NativeMessagingHosts/com.zyzoom.zdm.json"
    rm -f "${REAL_HOME}/.config/vivaldi/NativeMessagingHosts/com.zyzoom.zdm.json"
    rm -f "${REAL_HOME}/.mozilla/native-messaging-hosts/com.zyzoom.zdm.json"

    echo -e "${GREEN}✓ تمت إزالة البرنامج بنجاح.${NC}"
    exit 0
fi

# Step 1: Detect Distro & Dependencies
echo -e "${CYAN}>> [1/5] فحص التوزيعة والحزم المطلوبة...${NC}"
DISTRO="unknown"

if [ -f /etc/os-release ]; then
    . /etc/os-release
    DISTRO="$ID"
    DISTRO_LIKE="${ID_LIKE:-$ID}"
fi

echo -e "   النظام المكتشف: ${BOLD}${PRETTY_NAME:-$DISTRO}${NC}"

install_dependencies() {
    if command -v pacman &> /dev/null; then
        echo -e "   تثبيت مكتبات Arch Linux عبر pacman..."
        sudo pacman -S --needed --noconfirm webkit2gtk-4.1 libayatana-appindicator openssl base-devel 2>/dev/null || true
    elif command -v dnf &> /dev/null; then
        echo -e "   تثبيت مكتبات Fedora عبر dnf..."
        sudo dnf install -y webkit2gtk4.1 libayatana-appindicator-gtk3 openssl-devel 2>/dev/null || true
    elif command -v apt-get &> /dev/null; then
        echo -e "   تثبيت مكتبات Debian/Ubuntu عبر apt..."
        sudo apt-get update -qq
        sudo apt-get install -y libwebkit2gtk-4.1-0 libayatana-appindicator3-1 libssl-dev build-essential 2>/dev/null || true
    elif command -v zypper &> /dev/null; then
        echo -e "   تثبيت مكتبات openSUSE عبر zypper..."
        sudo zypper install -y libwebkit2gtk-4_1-0 libayatana-appindicator3-1 2>/dev/null || true
    fi
}

if [ "$1" == "--deps" ]; then
    install_dependencies
fi

# Step 2: Build Binaries if not already built
echo -e "${CYAN}>> [2/5] بناء وتجهيز ثنائيات ZDM (Rust Core)...${NC}"

cd "$PROJECT_ROOT"

if [ ! -f "target/release/zdm" ] || [ ! -f "target/release/zdm-host" ]; then
    echo -e "   بناء الحزمة المصغرة والواجهة المدمجة..."
    npm --prefix apps/desktop run build
    npx --prefix apps/desktop tauri build --no-bundle
    cargo build --release -p zdm-host
fi

# Ensure directories exist
mkdir -p "$BIN_DIR"
mkdir -p "$SHARE_DIR"
mkdir -p "$APPS_DIR"
mkdir -p "$AUTOSTART_DIR"
mkdir -p "${REAL_HOME}/.config/autostart"
mkdir -p "$ICONS_DIR/512x512/apps"
mkdir -p "$ICONS_DIR/256x256/apps"
mkdir -p "$ICONS_DIR/128x128/apps"
mkdir -p "$ICONS_DIR/32x32/apps"
mkdir -p "$ICONS_DIR/scalable/apps"

# Step 3: Install Binaries and Assets
echo -e "${CYAN}>> [3/5] تثبيت الملفات والأيقونات في النظام...${NC}"

cp "target/release/zdm" "${BIN_DIR}/zdm"
chmod +x "${BIN_DIR}/zdm"

cp "target/release/zdm-host" "${BIN_DIR}/zdm-host"
chmod +x "${BIN_DIR}/zdm-host"

# Copy extension files to share directory
cp -r "apps/extension" "${SHARE_DIR}/"

# Install Icons
cp "assets/icon-512.png" "${ICONS_DIR}/512x512/apps/zdm.png"
cp "assets/icon-256.png" "${ICONS_DIR}/256x256/apps/zdm.png"
cp "assets/icon-128.png" "${ICONS_DIR}/128x128/apps/zdm.png"
cp "assets/icon-32.png" "${ICONS_DIR}/32x32/apps/zdm.png"
cp "assets/logo.svg" "${ICONS_DIR}/scalable/apps/zdm.svg"

# Update icon cache if tool exists
if command -v gtk-update-icon-cache &> /dev/null; then
    gtk-update-icon-cache -f -t "$ICONS_DIR" 2>/dev/null || true
fi

# Create Application Desktop Entry
cat << EOF > "${APPS_DIR}/zdm.desktop"
[Desktop Entry]
Name=Zyzoom Download Manager
GenericName=Download Manager
Comment=Supersonic Multi-Segment Download Accelerator • Developed by Ramy Badraan
Exec=${BIN_DIR}/zdm %U
Icon=zdm
Terminal=false
Type=Application
Categories=Network;FileTransfer;P2P;GTK;
MimeType=application/x-bittorrent;x-scheme-handler/magnet;x-scheme-handler/zdm;
StartupNotify=true
Actions=AddUrl;

[Desktop Action AddUrl]
Name=إضافة رابط تنزيل جديد
Exec=${BIN_DIR}/zdm --add-url
EOF
chmod 644 "${APPS_DIR}/zdm.desktop"

# Step 4: Configure Autostart (Starts with System / Hyprland / Waybar)
echo -e "${CYAN}>> [4/5] ضبط التشغيل التلقائي مع إقلاع النظام (System Tray / Waybar)...${NC}"

cat << EOF > "${REAL_HOME}/.config/autostart/zdm.desktop"
[Desktop Entry]
Name=Zyzoom Download Manager (Tray Autostart)
Comment=Start ZDM minimized in system tray on login
Exec=${BIN_DIR}/zdm --minimized
Icon=zdm
Terminal=false
Type=Application
Categories=Network;
X-GNOME-Autostart-enabled=true
X-KDE-autostart-after=panel
EOF
chmod 644 "${REAL_HOME}/.config/autostart/zdm.desktop"
chown "$REAL_USER":"$REAL_USER" "${REAL_HOME}/.config/autostart/zdm.desktop" 2>/dev/null || true

# If root, also put in global autostart
if [ "$IS_ROOT" -eq 1 ]; then
    cp "${REAL_HOME}/.config/autostart/zdm.desktop" "${AUTOSTART_DIR}/zdm.desktop"
fi

# Step 5: Automatically Install Browser Extension for All Browsers
echo -e "${CYAN}>> [5/5] تثبيت إضافة المتصفح تلقائياً (Chrome / Brave / Edge / Firefox)...${NC}"

BROWSERS=(
    "google-chrome:${REAL_HOME}/.config/google-chrome"
    "chromium:${REAL_HOME}/.config/chromium"
    "brave:${REAL_HOME}/.config/BraveSoftware/Brave-Browser"
    "edge:${REAL_HOME}/.config/microsoft-edge"
    "vivaldi:${REAL_HOME}/.config/vivaldi"
)

# Chromium Native Messaging Manifest
CHROME_MANIFEST=$(cat << EOF
{
  "name": "com.zyzoom.zdm",
  "description": "Zyzoom Download Manager (ZDM) Native Messaging Host - Developed by Ramy Badraan",
  "path": "${BIN_DIR}/zdm-host",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://*/",
    "chrome-extension://com.zyzoom.zdm/"
  ]
}
EOF
)

EXTERNAL_EXT_MANIFEST=$(cat << EOF
{
  "external_path": "${SHARE_DIR}/extension"
}
EOF
)

for ENTRY in "${BROWSERS[@]}"; do
    BNAME="${ENTRY%%:*}"
    BDIR="${ENTRY##*:}"

    mkdir -p "${BDIR}/NativeMessagingHosts"
    echo "$CHROME_MANIFEST" > "${BDIR}/NativeMessagingHosts/com.zyzoom.zdm.json"

    mkdir -p "${BDIR}/External Extensions"
    echo "$EXTERNAL_EXT_MANIFEST" > "${BDIR}/External Extensions/zdm_extension.json"

    chown -R "$REAL_USER":"$REAL_USER" "${BDIR}/NativeMessagingHosts" 2>/dev/null || true
    chown -R "$REAL_USER":"$REAL_USER" "${BDIR}/External Extensions" 2>/dev/null || true
    echo -e "   ${GREEN}✓${NC} تم تفعيل الإضافة لمتصفح: ${BOLD}${BNAME}${NC}"
done

# Firefox Integration
FF_DIR="${REAL_HOME}/.mozilla"
mkdir -p "${FF_DIR}/native-messaging-hosts"
cat << EOF > "${FF_DIR}/native-messaging-hosts/com.zyzoom.zdm.json"
{
  "name": "com.zyzoom.zdm",
  "description": "Zyzoom Download Manager (ZDM) Native Messaging Host - Developed by Ramy Badraan",
  "path": "${BIN_DIR}/zdm-host",
  "type": "stdio",
  "allowed_extensions": [
    "zdm@zyzoom.com"
  ]
}
EOF
chown -R "$REAL_USER":"$REAL_USER" "${FF_DIR}/native-messaging-hosts" 2>/dev/null || true
echo -e "   ${GREEN}✓${NC} تم تفعيل الإضافة لمتصفح: ${BOLD}Firefox${NC}"

echo -e "\n${GREEN}${BOLD}============================================================${NC}"
echo -e "${GREEN}${BOLD}✓ تم تنصيب Zyzoom Download Manager (ZDM) بنجاح على جهازك!${NC}"
echo -e "${CYAN}------------------------------------------------------------${NC}"
echo -e "1. مسار التشغيل: ${BOLD}${BIN_DIR}/zdm${NC}"
echo -e "2. التشغيل التلقائي: ${BOLD}مفعل مع بدء النظام في شريط المهام / Waybar (Hyprland)${NC}"
echo -e "3. إضافة المتصفح: ${BOLD}مثبتة تلقائياً في جميع المتصفحات على النظام${NC}"
echo -e "4. أيقونة Waybar: تأكد من وجود وحدة ${BOLD}\"tray\"${NC} في شريط waybar لديك لتظهر أيقونة ZDM."
echo -e "\n${BLUE}Developed with pride by ${CYAN}${BOLD}Ramy Badraan${NC}\n"
exit 0

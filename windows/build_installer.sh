#!/usr/bin/env bash
# ==============================================================================
# Zyzoom Download Manager (ZDM) — Windows Installer Build Automation
# Developed by Ramy Badraan
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

echo ">> [1/4] تصريف واجهة الـ Frontend الإنتاجية..."
cd "$ROOT_DIR/apps/desktop"
npm run build

echo ">> [2/4] تصريف مضيف المتصفحات zdm-host.exe لنظام Windows..."
cd "$ROOT_DIR"
export PATH="$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
cargo xwin build --release --target x86_64-pc-windows-msvc -p zdm-host

echo ">> [3/4] تصريف محرك ZDM الرسمي zdm.exe لنظام Windows..."
cargo xwin build --release --target x86_64-pc-windows-msvc --manifest-path "$ROOT_DIR/apps/desktop/src-tauri/Cargo.toml"

echo ">> [4/4] حزم وتوليد ملف التنصيب الكامل Zyzoom_Download_Manager_Setup_v1.0.0.exe..."
cd "$SCRIPT_DIR"
makensis installer.nsi

echo ""
echo "=============================================================================="
echo "✓ تم توليد ملف التنصيب لويندوز بنجاح في:"
echo "  $ROOT_DIR/Zyzoom_Download_Manager_Setup_v1.0.0.exe"
echo "=============================================================================="

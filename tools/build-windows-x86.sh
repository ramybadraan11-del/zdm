#!/usr/bin/env bash
# ==============================================================================
# Build the 32-bit Windows binaries (x86 / i686) from Linux.
# Developed by Ramy Badraan
#
# Why this exists instead of `cargo xwin build`:
#   `cargo-xwin` decides which architectures to splat and ignores a cache that
#   contains only x86, so its link step fails with "could not open kernel32.lib".
#   Driving `cargo build` directly, with LIB/include paths pointing at a cache
#   prepared by `xwin` itself, is deterministic and repeatable.
#
# Usage: bash tools/build-windows-x86.sh [-p <crate>]...
# ==============================================================================
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

TARGET="i686-pc-windows-msvc"
CACHE_ROOT="$ROOT/.build/xwincache"
CACHE="$CACHE_ROOT/xwin"

# ---------------------------------------------------------------- SDK cache
if [ ! -f "$CACHE/sdk/lib/um/x86/kernel32.lib" ]; then
  echo ">> Preparing the x86 Windows SDK cache (one time, ~100 MB)"
  command -v xwin >/dev/null 2>&1 || {
    echo "xwin is required: cargo install xwin --locked" >&2
    exit 1
  }
  rm -rf "$CACHE_ROOT"
  mkdir -p "$CACHE_ROOT"
  xwin --accept-license --arch x86 splat --output "$CACHE" >/dev/null
fi

if [ ! -f "$CACHE/sdk/lib/um/x86/kernel32.lib" ]; then
  echo "the x86 SDK cache is incomplete at $CACHE" >&2
  exit 1
fi

INCLUDES="-imsvc $CACHE/crt/include -imsvc $CACHE/sdk/include/ucrt -imsvc $CACHE/sdk/include/um -imsvc $CACHE/sdk/include/shared"

# aws-lc-sys (pulled in by librqbit's SHA-1 backend) assembles x86 code with NASM.
# Without it the 32-bit build dies with "NASM command not found".
# Prefer a locally built NASM (see .build/nasm) so the 32-bit build works without
# root, then fall back to PATH.
if [ -x "$ROOT/.build/nasm/bin/nasm" ]; then
  export PATH="$ROOT/.build/nasm/bin:$PATH"
fi
if ! command -v nasm >/dev/null 2>&1; then
  echo "  ! NASM is required for the 32-bit build (aws-lc-sys assembles x86)."
  echo "    Arch:   sudo pacman -S nasm"
  echo "    Debian: sudo apt install nasm"
  echo "    Fedora: sudo dnf install nasm"
  echo "    Or build one locally: tools/fetch-nasm.sh"
  exit 1
fi

# ------------------------------------------------------------- toolchain
export CC_i686_pc_windows_msvc="clang-cl"
export CXX_i686_pc_windows_msvc="clang-cl"
export AR_i686_pc_windows_msvc="llvm-lib"
export CFLAGS_i686_pc_windows_msvc="--target=i686-pc-windows-msvc $INCLUDES"
export CXXFLAGS_i686_pc_windows_msvc="--target=i686-pc-windows-msvc $INCLUDES"
export CFLAGS_ia32_pc_windows_msvc="$CFLAGS_i686_pc_windows_msvc"
export CXXFLAGS_ia32_pc_windows_msvc="$CXXFLAGS_i686_pc_windows_msvc"
export BINDGEN_EXTRA_CLANG_ARGS="--target=i686-pc-windows-msvc $INCLUDES"

# rustc must use lld-link; there is no MSVC link.exe on Linux.
export RUSTFLAGS="\
 -C linker=lld-link \
 -C link-arg=/LIBPATH:$CACHE/crt/lib/x86 \
 -C link-arg=/LIBPATH:$CACHE/sdk/lib/um/x86 \
 -C link-arg=/LIBPATH:$CACHE/sdk/lib/ucrt/x86"

echo ">> Building for $TARGET"
cargo build --release --target "$TARGET" "$@"

echo
echo ">> Artifacts"
ls -lh "target/$TARGET/release/"*.exe 2>/dev/null | awk '{printf "   %-50s %s\n", $9, $5}'

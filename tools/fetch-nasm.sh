#!/usr/bin/env bash
# ==============================================================================
# ZDM NASM Fetcher
# Developed by Ramy Badraan
#
# The 32-bit Windows build needs NASM: librqbit's SHA-1 backend pulls in
# aws-lc-sys, which assembles x86 code. Distributions package NASM, but building
# it locally avoids needing root just to produce a 32-bit installer.
#
# Installs to .build/nasm so tools/build-windows-x86.sh finds it automatically.
#
# Usage: tools/fetch-nasm.sh
# ==============================================================================
set -euo pipefail

VERSION="2.16.03"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PREFIX="${ROOT}/.build/nasm"
URL="https://www.nasm.us/pub/nasm/releasebuilds/${VERSION}/nasm-${VERSION}.tar.xz"

if [ -x "${PREFIX}/bin/nasm" ]; then
  echo "  ✓ NASM already present: $("${PREFIX}/bin/nasm" -v)"
  exit 0
fi

echo ">> Fetching NASM ${VERSION}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -sfL "$URL" -o "${TMP}/nasm.tar.xz" || {
  echo "  ! download failed: ${URL}" >&2
  exit 1
}

tar xf "${TMP}/nasm.tar.xz" -C "$TMP" --strip-components=1
(
  cd "$TMP"
  ./configure --prefix="$PREFIX" >/dev/null
  make -j"$(nproc 2>/dev/null || echo 2)" >/dev/null
  make install >/dev/null
)

"${PREFIX}/bin/nasm" -v
echo "  ✓ installed under ${PREFIX}"

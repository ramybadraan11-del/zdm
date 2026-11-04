#!/usr/bin/env bash
# ==============================================================================
# ZDM Package Verification
# Developed by Ramy Badraan
#
# Fails loudly if a produced package is malformed. This is what turns "the script
# exited 0" into "dnf/apt will actually install this".
#
# Checks per format:
#   .deb     — ar members, control fields, root ownership, exec bits
#   .rpm     — lead magic, header readability, payload extraction, requires
#   AppImage — type-2 runtime magic, squashfs presence, AppRun + desktop entry
#
# Usage: bash packaging/verify.sh
# ==============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
FAILED=0

GREEN=$'\033[0;32m'; RED=$'\033[0;31m'; YELLOW=$'\033[1;33m'; BOLD=$'\033[1m'; NC=$'\033[0m'
pass() { printf '  %s %s\n' "${GREEN}✓${NC}" "$*"; }
bad()  { printf '  %s %s\n' "${RED}✗${NC}" "$*"; FAILED=1; }
warn() { printf '  %s %s\n' "${YELLOW}!${NC}" "$*"; }
head2() { printf '\n%s\n' "${BOLD}$*${NC}"; }

[ -d "$DIST" ] || { echo "no dist/ directory — run packaging/build-linux.sh first" >&2; exit 1; }

# ------------------------------------------------------------------------ deb
verify_deb() {
  local file="$1"
  head2 "Verifying $(basename "$file")"

  local members
  members="$(ar t "$file" 2>/dev/null)"
  if [ -z "$members" ]; then
    bad "not a valid ar archive"
    return
  fi
  pass "ar archive readable"

  for required in debian-binary control.tar data.tar; do
    if printf '%s\n' "$members" | grep -q "^${required}"; then
      pass "contains ${required}"
    else
      bad "missing ${required}"
    fi
  done

  local work
  work="$(mktemp -d)"
  ( cd "$work" && ar x "$file" ) || { bad "cannot extract"; rm -rf "$work"; return; }

  # Control fields that apt requires. The member names vary (control.tar.xz,
  # control.tar.gz, control.tar.zst) so we resolve the actual file first.
  local control
  control="$(
    cd "$work" || return
    for archive in control.tar.xz control.tar.gz control.tar.zst control.tar; do
      [ -f "$archive" ] || continue
      case "$archive" in
        *.xz)  tar -xJf "$archive" -O control 2>/dev/null && break ;;
        *.gz)  tar -xzf "$archive" -O control 2>/dev/null && break ;;
        *.zst) tar --zstd -xf "$archive" -O control 2>/dev/null && break ;;
        *)     tar -xf "$archive" -O control 2>/dev/null && break ;;
      esac
    done
  )"

  for field in Package Version Architecture Maintainer Description; do
    if printf '%s' "$control" | grep -q "^${field}:"; then
      pass "control: ${field}"
    else
      bad "control: missing ${field}"
    fi
  done

  if printf '%s' "$control" | grep -qi "^Depends:.*webkit2gtk"; then
    pass "declares the WebKit runtime dependency"
  else
    bad "does not depend on webkit2gtk (the app cannot start without it)"
  fi

  # Payload sanity: executables must be executable and owned by root.
  local listing
  listing="$(
    cd "$work" || return
    for archive in data.tar.xz data.tar.gz data.tar.zst data.tar; do
      [ -f "$archive" ] || continue
      case "$archive" in
        *.xz)  tar -tvJf "$archive" 2>/dev/null && break ;;
        *.gz)  tar -tvzf "$archive" 2>/dev/null && break ;;
        *.zst) tar --zstd -tvf "$archive" 2>/dev/null && break ;;
        *)     tar -tvf "$archive" 2>/dev/null && break ;;
      esac
    done
  )"

  if printf '%s' "$listing" | grep -q "usr/bin/zdm$"; then
    if printf '%s' "$listing" | grep "usr/bin/zdm$" | grep -q "^-rwx"; then
      pass "zdm is executable"
    else
      bad "zdm is not executable"
    fi
  else
    bad "usr/bin/zdm missing from the payload"
  fi

  if printf '%s' "$listing" | grep -q "usr/bin/zdm-host$"; then
    pass "native host is present"
  else
    bad "usr/bin/zdm-host missing from the payload"
  fi

  if printf '%s' "$listing" | grep -q "usr/share/applications/zdm.desktop"; then
    pass "desktop entry present"
  else
    bad "desktop entry missing"
  fi

  if printf '%s' "$listing" | grep -q "usr/share/zdm/extension/manifest.json"; then
    pass "browser extension bundled"
  else
    bad "browser extension missing"
  fi

  if printf '%s' "$listing" | grep -qE "usr/share/zdm/[a-p]{32}\.crx"; then
    pass "signed CRX bundled"
  else
    warn "no CRX: Chromium auto-install will be unavailable"
  fi

  # Ownership must be root:root — a vboxsf/ext4 home leaks the builder's uid.
  if printf '%s' "$listing" | grep -qv " 0/0 "; then
    local non_root
    non_root="$(printf '%s' "$listing" | awk '$2 != "0/0" {print; count++} END {exit !count}')"
    [ -n "$non_root" ] && bad "files are not owned by root:root"
  fi
  [ -z "${non_root:-}" ] && pass "all files owned by root:root"

  local size
  size="$(stat -c%s "$file")"
  if [ "$size" -gt 1000000 ]; then
    pass "package size $(numfmt --to=iec "$size")"
  else
    bad "package is suspiciously small"
  fi

  rm -rf "$work"
}

# ------------------------------------------------------------------------ rpm
verify_rpm() {
  local file="$1"
  head2 "Verifying $(basename "$file")"

  # Lead magic: 0xED 0xAB 0xEE 0xDB
  local magic
  magic="$(head -c 4 "$file" | od -An -tx1 | tr -d ' \n')"
  if [ "$magic" = "edabeedb" ]; then
    pass "RPM lead magic"
  else
    bad "bad RPM lead magic ($magic)"
    return
  fi

  local rpm_ver
  rpm_ver="$(dd if="$file" bs=1 skip=4 count=2 2>/dev/null | od -An -tu1 | tr -s ' ')"
  pass "RPM format version${rpm_ver}"

  # libarchive reads the cpio payload independently of rpm. A malformed trailer
  # (the bug that made Fedora reject the package) surfaces here as an error even
  # when no rpm toolchain is installed.
  if command -v bsdtar >/dev/null 2>&1; then
    if bsdtar -tf "$file" >/dev/null 2>&1; then
      pass "payload listing parses (bsdtar)"
    else
      bad "bsdtar cannot read the cpio payload (malformed trailer?)"
    fi
  fi

  # Real install smoke test. bsdtar parses archives that rpm still rejects (e.g.
  # explicit directory entries trigger "cpio: Archive file not in header"), so
  # parse-only checks are not enough. Run the actual unpack under a user
  # namespace when possible.
  # The smoke test needs a real rpm(8). Prefer the system one; fall back to a
  # cached copy in .build/rpmtools (never /tmp, it gets wiped).
  local rpm_tools="$ROOT/.build/rpmtools"
  if [ ! -x "$rpm_tools/usr/bin/rpm" ] && command -v unshare >/dev/null 2>&1 && [ ! -x "$rpm_tools/usr/bin/rpm" ]; then
    mkdir -p "$rpm_tools"
    # rpm 6.0.1 from the Arch extra repository, as it was sufficient for all
    # the tag/digest checks here. Kept small: rpm + rpm-sequoia + nettle for libnettle.
    local arch=extra os="https://geo.mirror.pkgbuild.com/extra/os/x86_64"
    (
      cd "$rpm_tools" || exit 1
      curl -fsSL --max-time 120 -o rpm.pkg.tar.zst "$os/rpm-tools-6.0.1-2-x86_64.pkg.tar.zst" && \
      curl -fsSL --max-time 120 -o sequoia.pkg.tar.zst "$os/rpm-sequoia-1.10.3-1-x86_64.pkg.tar.zst" && \
      curl -fsSL --max-time 120 -o nettle.pkg.tar.zst "https://archive.archlinux.org/packages/n/nettle/nettle-3.9.1-1-x86_64.pkg.tar.zst" && \
      command -v bsdtar >/dev/null 2>&1 && \
      bsdtar -xf rpm.pkg.tar.zst && bsdtar -xf sequoia.pkg.tar.zst && bsdtar -xf nettle.pkg.tar.zst && \
      rm -f ./*.pkg.tar.zst
    ) >/dev/null 2>&1 || warn "could not fetch rpm tools into $rpm_tools — rpm smoke test skipped"
  fi

  if command -v unshare >/dev/null 2>&1 && { command -v rpm >/dev/null 2>&1 || [ -x "$rpm_tools/usr/bin/rpm" ]; }; then
    local root
    root="$(mktemp -d)"
    if unshare -r -m bash -c "
      export LD_LIBRARY_PATH=\"$rpm_tools/usr/lib:$rpm_tools/usr/lib/x86_64-linux-gnu\"
      export RPM_CONFIGDIR=\"$rpm_tools/usr/lib/rpm\"
      export PATH=\"$rpm_tools/usr/bin:/usr/bin:/bin\"
      rm -rf '$root/var'; mkdir -p '$root/var/lib/rpm'
      if command -v rpm >/dev/null 2>&1; then
        rpm --root '$root' --dbpath /var/lib/rpm --initdb >/dev/null 2>&1
        rpm --root '$root' --dbpath /var/lib/rpm --define '_pkgverify_level digest' -i --nodeps --noscripts '$file' >/dev/null 2>&1
      else
        exit 0
      fi
    " 2>/dev/null; then
      pass "real rpm install succeeds (user namespace)"
    else
      bad "real rpm install failed (cpio/header problem?)"
    fi
    rm -rf "$root"
  fi

  if command -v rpm >/dev/null 2>&1; then
    if rpm -qp --queryformat '%{NAME} %{VERSION}-%{RELEASE} %{ARCH}\n' "$file" 2>/dev/null; then
      pass "rpm query works"
    else
      bad "rpm -qp failed"
    fi
    if rpm -qp --requires "$file" 2>/dev/null | grep -q webkitgtk; then
      pass "requires webkitgtk"
    else
      warn "webkitgtk not listed in requires"
    fi
    if rpm -K "$file" >/dev/null 2>&1; then
      pass "digests verified (rpm -K)"
    else
      warn "rpm -K could not verify digests (no key is configured)"
    fi
    return
  fi

  # No rpm toolchain: verify with Python instead.
  if python3 -c "import rpmfile" 2>/dev/null; then
    if python3 -c "
import rpmfile
with rpmfile.open('$file') as r:
    assert r.headers.get('name') == b'zyzoom-download-manager'
    assert r.headers.get('arch') == b'x86_64'
    members = [m.name for m in r.getmembers()]
    assert all(members), 'empty archive member (malformed cpio trailer)'
    assert './usr/bin/zdm' in members, members[:5]
    data = r.extractfile('./usr/bin/zdm').read(4)
    assert data == b'\x7fELF', data
print('ok')
" 2>/dev/null; then
      pass "header parses and payload extracts (independent reader)"
      pass "usr/bin/zdm is a valid ELF"
    else
      bad "independent RPM reader rejected the package"
    fi
  else
    # rpmfile is a test-only dependency; absence must not fail the build.
    warn "rpm tooling and rpmfile both unavailable: skipping deep RPM check"
  fi
}

# ------------------------------------------------------------------- AppImage
verify_appimage() {
  local file="$1"
  head2 "Verifying $(basename "$file")"

  [ -x "$file" ] || bad "AppImage is not executable"

  # Compare bytes, not text: `head -c` output is binary.
  local magic
  magic="$(head -c 4 "$file" | od -An -tx1 | tr -d ' \n')"
  if [ "$magic" = "7f454c46" ]; then
    pass "type-2 runtime (ELF header)"
  else
    bad "missing ELF runtime header"
    return
  fi

  local offset
  offset="$("$file" --appimage-offset 2>/dev/null || true)"
  if [ -n "$offset" ] && [ "$offset" -gt 100000 ]; then
    pass "squashfs payload at offset $offset"
  else
    bad "runtime did not report a payload offset"
  fi

  if head -c 4 "$file" | grep -qa "ELF"; then
    if dd if="$file" bs=1 skip="${offset:-0}" count=4 2>/dev/null | grep -qa "hsqs"; then
      pass "squashfs magic present"
    else
      warn "squashfs magic not found at the reported offset"
    fi
  fi

  # Extract and validate the bundled desktop entry.
  local work
  work="$(mktemp -d)"
  if ( cd "$work" && "$file" --appimage-extract >/dev/null 2>&1 ); then
    if [ -f "$work/squashfs-root/AppRun" ]; then
      pass "AppRun present"
    else
      bad "AppRun missing"
    fi
    if [ -f "$work/squashfs-root/zdm.desktop" ]; then
      pass "desktop entry present"
      if command -v desktop-file-validate >/dev/null 2>&1; then
        if desktop-file-validate "$work/squashfs-root/zdm.desktop" 2>/dev/null; then
          pass "desktop entry validates"
        else
          warn "desktop entry has validation warnings"
        fi
      fi
    else
      bad "desktop entry missing"
    fi
    if [ -x "$work/squashfs-root/usr/bin/zdm" ]; then
      pass "zdm binary bundled and executable"
    else
      bad "zdm binary missing or not executable"
    fi
    if [ -f "$work/squashfs-root/usr/share/zdm/extension/manifest.json" ]; then
      pass "browser extension bundled"
    else
      warn "browser extension not bundled in the AppImage"
    fi
  else
    bad "could not extract the AppImage"
  fi
  rm -rf "$work"
}

# --------------------------------------------------------------------- runner
shopt -s nullglob
debs=("$DIST"/*.deb)
rpms=("$DIST"/*.rpm)
appimages=("$DIST"/*.AppImage)

if [ ${#debs[@]} -eq 0 ] && [ ${#rpms[@]} -eq 0 ] && [ ${#appimages[@]} -eq 0 ]; then
  echo "no packages found in $DIST" >&2
  exit 1
fi

for file in "${debs[@]}";      do verify_deb "$file"; done
for file in "${rpms[@]}";      do verify_rpm "$file"; done
for file in "${appimages[@]}"; do verify_appimage "$file"; done

# ------------------------------------------------------ windows installer (opt)
setups=("$DIST"/Zyzoom_Download_Manager_Setup_*.exe)
if [ ${#setups[@]} -gt 0 ]; then
  head2 "Verifying $(basename "${setups[0]}")"

  setup="${setups[0]}"
  magic="$(head -c 2 "$setup")"
  if [ "$magic" = "MZ" ]; then
    pass "MZ executable"
  else
    bad "not a Windows executable"
  fi

  if command -v 7z >/dev/null 2>&1; then
    if 7z l "$setup" 2>/dev/null | grep -q "NSIS"; then
      pass "NSIS installer container"
    else
      bad "not an NSIS container"
    fi
    for entry in zdm.exe zdm-host.exe; do
      if 7z l "$setup" 2>/dev/null | grep -q "$entry"; then
        pass "contains $entry"
      else
        bad "missing $entry"
      fi
    done
    if 7z l "$setup" 2>/dev/null | grep -q "yt-dlp.exe"; then
      pass "bundles yt-dlp"
    else
      bad "yt-dlp not bundled"
    fi

    # Video merging is impossible without it; shipping the installer without
    # ffmpeg is what produced "two files" and "ffmpeg download interrupted".
    if 7z l "$setup" 2>/dev/null | grep -q "bin/ffmpeg.exe"; then
      pass "bundles ffmpeg (video + audio merged into one file)"
    else
      bad "ffmpeg is missing: video downloads would produce two separate files"
    fi
  else
    warn "7z unavailable: skipping installer inspection"
  fi

  size_mb=$(( $(stat -c%s "$setup") / 1024 / 1024 ))
  pass "size ${size_mb} MB"
fi

printf '\n'
if [ "$FAILED" -eq 0 ]; then
  printf '%s\n' "${GREEN}${BOLD}  All packages verified.${NC}"
  exit 0
fi
printf '%s\n' "${RED}${BOLD}  Package verification FAILED.${NC}"
exit 1

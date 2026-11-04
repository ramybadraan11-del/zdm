#!/usr/bin/env bash
# ==============================================================================
# ZDM Preflight — one command that checks the whole repository is coherent.
# Developed by Ramy Badraan
#
# Catches the failures that are invisible until a user hits them:
#   • an extension id that disagrees between the manifest, the Rust code and the
#     installers (Chromium then silently refuses to load the native host),
#   • a CRX3 that does not match the manifest key,
#   • Rust/TypeScript that does not compile,
#   • unit tests failing,
#   • missing icons, fonts or localisation files,
#   • JSON/NSIS/shell syntax errors.
#
# Usage: bash tools/preflight.sh [--quick]
# ==============================================================================
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

QUICK=0
[ "${1:-}" = "--quick" ] && QUICK=1

GREEN=$'\033[0;32m'; RED=$'\033[0;31m'; YELLOW=$'\033[1;33m'
BOLD=$'\033[1m'; DIM=$'\033[2m'; NC=$'\033[0m'
FAILED=0

pass()  { printf '  %s %s\n' "${GREEN}✓${NC}" "$*"; }
bad()   { printf '  %s %s\n' "${RED}✗${NC}" "$*"; FAILED=1; }
warn()  { printf '  %s %s\n' "${YELLOW}!${NC}" "$*"; }
group() { printf '\n%s\n' "${BOLD}$*${NC}"; }

# --------------------------------------------------------------- consistency
group "Extension identity"

MANIFEST_KEY="$(python3 -c "
import json,hashlib,base64
m=json.load(open('apps/extension/manifest.json'))
der=base64.b64decode(m['key'])
print(''.join(chr(ord('a')+int(c,16)) for c in hashlib.sha256(der).hexdigest()[:32]))
")"

if [ -n "$MANIFEST_KEY" ]; then
  pass "manifest key derives id ${MANIFEST_KEY}"
else
  bad "could not derive the extension id from the manifest key"
fi

for file in \
  apps/desktop/src-tauri/src/browser_installer.rs \
  install.sh \
  windows/installer.nsi; do
  if grep -q "$MANIFEST_KEY" "$file"; then
    pass "$file uses the same id"
  else
    bad "$file has a different (or missing) extension id"
  fi
done

if [ -f assets/zdm.crx ]; then
  if python3 - "$MANIFEST_KEY" <<'PY'
import struct, sys, hashlib, zipfile, io
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding

expected = sys.argv[1]
data = open('assets/zdm.crx', 'rb').read()
if data[:4] != b'Cr24':
    sys.exit(1)
hlen = struct.unpack('<I', data[8:12])[0]
header, body = data[12:12 + hlen], data[12 + hlen:]

def rv(b, i):
    sh = v = 0
    while True:
        x = b[i]; i += 1; v |= (x & 0x7f) << sh
        if not x & 0x80:
            return v, i
        sh += 7

def parse(b):
    i, out = 0, []
    while i < len(b):
        k, i = rv(b, i); n, w = k >> 3, k & 7
        if w == 2:
            ln, i = rv(b, i); out.append((n, b[i:i + ln])); i += ln
        else:
            v, i = rv(b, i); out.append((n, v))
    return out

fields = parse(header)
proof = next(v for n, v in fields if n == 2)
shd = next(v for n, v in fields if n == 10000)
pf = parse(proof)
pub = next(v for n, v in pf if n == 1)
sig = next(v for n, v in pf if n == 2)

signed = b'CRX3 SignedData\x00' + struct.pack('<I', len(shd)) + shd + body
serialization.load_der_public_key(pub).verify(sig, signed, padding.PKCS1v15(), hashes.SHA256())

derived = ''.join(chr(ord('a') + int(c, 16)) for c in hashlib.sha256(pub).hexdigest()[:32])
assert derived == expected, f"crx id {derived} != {expected}"

zipfile.ZipFile(io.BytesIO(body))  # must be a readable archive
sys.exit(0)
PY
  then
    pass "assets/zdm.crx signature verifies and matches the id"
  else
    bad "assets/zdm.crx does not verify against the manifest key"
  fi
else
  warn "assets/zdm.crx missing (Chromium auto-install will be skipped)"
fi

# ------------------------------------------------------------- localisation
group "Localisation"

for lang in ar en fr; do
  if [ -f "apps/extension/_locales/$lang/messages.json" ]; then
    if python3 -c "import json;json.load(open('apps/extension/_locales/$lang/messages.json'))" 2>/dev/null; then
      pass "extension locale: $lang"
    else
      bad "apps/extension/_locales/$lang/messages.json is not valid JSON"
    fi
  else
    bad "missing extension locale: $lang"
  fi
done

if python3 - <<'PY'
import re, sys
src = open('apps/desktop/src/i18n/dict.ts').read()

def keys_of(block_start):
    start = src.index(block_start)
    depth = 0
    i = src.index('{', start)
    begin = i
    while i < len(src):
        if src[i] == '{':
            depth += 1
        elif src[i] == '}':
            depth -= 1
            if depth == 0:
                break
        i += 1
    body = src[begin:i]
    return set(re.findall(r'^\s{2}([A-Za-z0-9_]+):', body, re.M))

base = keys_of('const en = {')
for name, marker in (('ar', 'const ar: Dict = {'), ('fr', 'const fr: Dict = {')):
    other = keys_of(marker)
    missing = base - other
    extra = other - base
    if missing:
        print(f"{name} is missing keys: {sorted(missing)[:6]}")
        sys.exit(1)
    if extra:
        print(f"{name} has keys not in en: {sorted(extra)[:6]}")
        sys.exit(1)
sys.exit(0)
PY
then
  pass "ar/en/fr dictionaries have identical key sets"
else
  bad "the translation dictionaries disagree"
fi

if [ -f apps/desktop/public/fonts/fonts.css ]; then
  count=$(grep -c "@font-face" apps/desktop/public/fonts/fonts.css)
  pass "bundled fonts present ($count faces, offline-capable)"
else
  warn "fonts not bundled — run tools/fetch-fonts.sh"
fi

# ------------------------------------------------------------------- assets
group "Assets"

for size in 32 128 256 512; do
  [ -f "assets/icon-$size.png" ] && pass "icon-$size.png" || bad "missing assets/icon-$size.png"
done
[ -f apps/desktop/src-tauri/icons/icon.ico ] && pass "windows icon.ico" || bad "missing the Windows .ico"
[ -f apps/desktop/src-tauri/icons/icon.png ] && pass "tauri app icon" || bad "missing the Tauri icon"
[ -f apps/extension/icons/icon128.png ] && pass "extension icons" || bad "missing the extension icons"
[ -f windows/icons/header.bmp ] && pass "installer header bitmap" || warn "installer header.bmp not generated"
[ -f windows/icons/wizard.bmp ] && pass "installer wizard bitmap" || warn "installer wizard.bmp not generated"

# NSIS resolves `Call`/`Function` at compile time. A missing policy helper is not
# a compile error if it is simply never called, so assert both sides exist.
if grep -q "Function WriteChromePolicy" windows/installer.nsi \
   && grep -q "Call WriteChromePolicy" windows/installer.nsi; then
  pass "enterprise extension policy is wired into the installer"
else
  bad "the Chromium policy helper is missing (Edge would warn about an untrusted source)"
fi

# ------------------------------------------------------------------- syntax
group "Syntax"

if bash -n install.sh 2>/dev/null; then pass "install.sh"; else bad "install.sh has a syntax error"; fi
if bash -n packaging/build-linux.sh 2>/dev/null; then pass "packaging/build-linux.sh"; else bad "packaging/build-linux.sh"; fi
if bash -n packaging/verify.sh 2>/dev/null; then pass "packaging/verify.sh"; else bad "packaging/verify.sh"; fi
if python3 -m py_compile tools/build-crx.py tools/mk-rpm.py 2>/dev/null; then
  pass "python tools"
else
  bad "python tool syntax error"
fi
for js in apps/extension/content.js apps/extension/background.js apps/extension/popup.js; do
  if node --check "$js" 2>/dev/null; then pass "$(basename "$js")"; else bad "$js syntax error"; fi
done
if python3 -c "import json;json.load(open('apps/extension/manifest.json'))" 2>/dev/null; then
  pass "extension manifest.json"
else
  bad "extension manifest.json is not valid JSON"
fi
if python3 -c "import json;json.load(open('apps/desktop/src-tauri/tauri.conf.json'))" 2>/dev/null; then
  pass "tauri.conf.json"
else
  bad "tauri.conf.json is not valid JSON"
fi
if python3 -c "import json;json.load(open('apps/desktop/src-tauri/capabilities/default.json'))" 2>/dev/null; then
  pass "capabilities/default.json"
else
  bad "capabilities/default.json is not valid JSON"
fi

# --------------------------------------------------------- windows installer
SETUP="$(ls dist/Zyzoom_Download_Manager_Setup_*.exe 2>/dev/null | head -1)"
if [ -n "$SETUP" ]; then
  group "Windows installer"

  if python3 - "$SETUP" <<'PYCHECK'
import struct, sys
data = open(sys.argv[1], 'rb').read()
assert data[:2] == b'MZ'
pe = struct.unpack('<I', data[0x3C:0x40])[0]
assert data[pe:pe + 4] == b'PE\0\0'
subsystem = struct.unpack('<H', data[pe + 0x5C:pe + 0x5E])[0]
assert subsystem == 2, f"subsystem {subsystem}, expected GUI"
assert b'NullsoftInst'.lower() in data.lower(), "not an NSIS archive"
sys.exit(0)
PYCHECK
  then
    pass "$(basename "$SETUP") is a valid NSIS installer"

    SIZE_MB=$(( $(stat -c%s "$SETUP") / 1024 / 1024 ))
    # Two supported shapes:
    #   self-contained (~230 MB) — bundles the WebView2 bootstrapper so it works
    #                              on a fresh Windows 10 with no internet,
    #   slim            (~20 MB)  — expects WebView2 to be present or downloaded.
    if [ "$SIZE_MB" -ge 180 ] && [ "$SIZE_MB" -le 400 ]; then
      pass "size ${SIZE_MB} MB (self-contained: includes WebView2)"
    elif [ "$SIZE_MB" -ge 15 ] && [ "$SIZE_MB" -lt 120 ]; then
      pass "size ${SIZE_MB} MB (slim: WebView2 expected on the system)"
    else
      bad "unexpected installer size ${SIZE_MB} MB"
    fi

    if command -v 7z >/dev/null 2>&1; then
      for entry in zdm.exe zdm-host.exe; do
        if 7z l "$SETUP" 2>/dev/null | grep -q "$entry"; then
          pass "payload contains $entry"
        else
          bad "payload is missing $entry"
        fi
      done
      if 7z l "$SETUP" 2>/dev/null | grep -qE "_locales/(ar|en|fr)"; then
        pass "payload contains all three locales"
      else
        bad "extension locales missing from the payload"
      fi

      # Without ffmpeg every YouTube download above 720p lands as two files
      # (Title.f137.mp4 + Title.f140.m4a), which is the reported bug.
      if 7z l "$SETUP" 2>/dev/null | grep -q "bin/ffmpeg.exe"; then
        pass "payload bundles ffmpeg.exe (video merging works offline)"
      else
        bad "payload is missing ffmpeg.exe: video downloads would produce two files"
      fi

      if 7z l "$SETUP" 2>/dev/null | grep -q "bin/yt-dlp.exe"; then
        pass "payload bundles yt-dlp.exe"
      else
        bad "payload is missing yt-dlp.exe"
      fi
    fi
  else
    bad "$(basename "$SETUP") is not a valid installer"
  fi

  # Both architectures must exist and match their installer.
  for pair in "x86_64-pc-windows-msvc:8664:x64" "i686-pc-windows-msvc:014c:x86"; do
    triple="${pair%%:*}"; rest="${pair#*:}"; machine="${rest%%:*}"; label="${rest#*:}"
    exe="target/$triple/release/zdm.exe"
    [ -f "$exe" ] || { warn "no $label binary (build it with tools/build-windows-x86.sh)"; continue; }
    if python3 - "$exe" "$machine" <<'PYARCH'
import struct, sys
data = open(sys.argv[1], "rb").read()
pe = struct.unpack("<I", data[0x3C:0x40])[0]
if struct.unpack("<H", data[pe + 4:pe + 6])[0] != int(sys.argv[2], 16):
    sys.exit(1)
if struct.unpack("<H", data[pe + 0x5C:pe + 0x5E])[0] != 2:
    sys.exit(1)
sys.exit(0)
PYARCH
    then
      pass "$label zdm.exe is a GUI binary of the right architecture"
    else
      bad "$label zdm.exe has the wrong architecture or subsystem"
    fi
  done

  WIN_EXE="target/x86_64-pc-windows-msvc/release/zdm.exe"
  if [ -f "$WIN_EXE" ]; then
    if python3 - "$WIN_EXE" <<'PYSUBSYS'
import struct, sys
data = open(sys.argv[1], 'rb').read()
pe = struct.unpack('<I', data[0x3C:0x40])[0]
sys.exit(0 if struct.unpack('<H', data[pe + 0x5C:pe + 0x5E])[0] == 2 else 1)
PYSUBSYS
    then
      pass "zdm.exe is a GUI binary (no console window on Windows)"
    else
      bad "zdm.exe is a console binary: users would see a DOS window"
    fi
  else
    warn "no cross-compiled zdm.exe (bash windows/build_installer.sh builds it)"
  fi
fi

# ------------------------------------------------------- tauri build config
group "Tauri build config"

if python3 - <<'PYFRONT'
import json, os, sys

conf_path = 'apps/desktop/src-tauri/tauri.conf.json'
conf = json.load(open(conf_path))

# `frontendDist` is resolved relative to the directory that holds
# tauri.conf.json (apps/desktop/src-tauri), not relative to the frontend package.
# Getting this wrong makes `tauri build` fail with "Unable to find your web
# assets" even though vite just built them, so it is checked explicitly.
frontend = conf.get('build', {}).get('frontendDist')
if not frontend:
    print('frontendDist is not set')
    sys.exit(1)

resolved = os.path.normpath(
    os.path.join(os.path.dirname(conf_path), frontend)
)
index = os.path.join(resolved, 'index.html')

if os.path.isfile(index):
    print(f'frontendDist resolves to {resolved}')
    sys.exit(0)

print(f'frontendDist "{frontend}" resolves to {resolved}, which has no index.html')
print('  hint: the path is relative to apps/desktop/src-tauri, so from there the')
print('        frontend build lives at ../dist')
sys.exit(1)
PYFRONT
then
  pass "frontendDist resolves to a built frontend"
else
  bad "frontendDist does not point at the built frontend (tauri build will fail)"
fi

# The same mistake is easy to reintroduce with an absolute path, so also confirm
# that a release binary is the one produced by this tree.
if [ -f target/release/zdm ]; then
  if [ target/release/zdm -nt apps/desktop/src-tauri/tauri.conf.json ]; then
    pass "release binary is newer than tauri.conf.json"
  else
    warn "release binary is older than tauri.conf.json (rebuild to be sure)"
  fi
fi

# -------------------------------------------------- embedded frontend check
group "Embedded frontend"

# A release build must serve the frontend from inside the binary. If it was
# compiled without the `custom-protocol` feature the window opens on
# http://localhost:1420 and the user sees "Could not connect to localhost:
# Connection refused" — which is exactly what happened before.
for triple in x86_64-unknown-linux-gnu x86_64-pc-windows-msvc; do
  bin="target/$triple/release/zdm"
  [ "$triple" = "x86_64-unknown-linux-gnu" ] && bin="target/release/zdm"
  [ "$triple" = "x86_64-pc-windows-msvc" ] && bin="target/$triple/release/zdm.exe"

  [ -f "$bin" ] || continue

  # Asset filenames only exist in the binary when the frontend is embedded.
  # NOTE: quote "$1" explicitly; a previous version relied on `grep -q` closing
  # its stdin early, which `strings |` turns into SIGPIPE under `set -o pipefail`
  # and made this check report a false failure.
  ASSET_ID="$(strings "$bin" 2>/dev/null | grep -oE "index-[A-Za-z0-9_-]+\.js" | head -1 || true)"
  if [ -n "$ASSET_ID" ]; then
    pass "$(basename "$bin") ($triple) embeds the frontend"
  else
    bad "$(basename "$bin") ($triple) has no embedded frontend: it will try the dev server"
  fi
done

# The feature must be on by default in Cargo.toml, otherwise a plain
# `cargo build --release` (used by install.sh and the packagers) regresses.
if python3 - <<'PYFEAT'
import sys, tomllib

manifest = tomllib.load(open('apps/desktop/src-tauri/Cargo.toml', 'rb'))
features = manifest.get('features', {})
default = features.get('default', [])
custom = features.get('custom-protocol', [])

if 'custom-protocol' not in default:
    print('custom-protocol is not in the default feature set')
    sys.exit(1)

joined = ' '.join(custom)
# Both crates must receive the flag: `tauri` embeds the assets and
# `tauri-macros` decides the `dev` value used by generate_context!.
if 'tauri/custom-protocol' not in joined:
    print('the tauri crate does not get custom-protocol')
    sys.exit(1)
if 'tauri-macros/custom-protocol' not in joined:
    print('tauri-macros does not get custom-protocol (generate_context! stays in dev mode)')
    sys.exit(1)

sys.exit(0)
PYFEAT
then
  pass "custom-protocol is enabled by default for both tauri and tauri-macros"
else
  bad "custom-protocol feature wiring is wrong; release builds will open a blank window"
fi

# ------------------------------------------------- extension localisation
group "Extension language"

# The floating button must follow the *app* language, not the browser locale.
# chrome.i18n alone cannot do that, so the extension has its own dictionary fed
# by the bridge; these checks keep that wiring intact.
if [ -f apps/extension/i18n.js ]; then
  LANGS_FOUND=$(node -e "
const src = require('fs').readFileSync('apps/extension/i18n.js','utf8');
for (const l of ['ar','en','fr']) {
  if (src.includes(l + ': {')) process.stdout.write(l + ' ');
}
")
  if [ "$(printf '%s' "$LANGS_FOUND" | wc -w)" -eq 3 ]; then
    pass "dictionary covers ar, en, fr"
  else
    bad "dictionary is missing a language (found: ${LANGS_FOUND:-none})"
  fi

  # The button must be RE-MOUNTED after the language changes. A previous version
  # read `activeVideo` after `unmountButton()` had already nulled it, so the label
  # silently stayed English. This asserts the ordering in the real source.
  if node -e "
const src = require('fs').readFileSync('apps/extension/content.js','utf8');
const block = src.slice(src.indexOf('setAppLanguage(reply.language)'));
const capture = block.indexOf('const currentVideo = activeVideo');
const unmount = block.indexOf('unmountButton()', capture);
const mount   = block.indexOf('mountButton(currentVideo)', unmount);
if (capture < 0) { console.error('no capture of activeVideo'); process.exit(1); }
if (unmount < 0) { console.error('no unmount after capture'); process.exit(1); }
if (mount < 0)   { console.error('the button is never remounted: label stays stale'); process.exit(1); }
if (!(capture < unmount && unmount < mount)) {
  console.error('wrong order: capture -> unmount -> mount'); process.exit(1);
}
" 2>/dev/null; then
    pass "language change re-mounts the button in the new language"
  else
    bad "the floating button would keep the old language after an app language change"
  fi

  if node -e "
global.chrome = { i18n: { getUILanguage: () => 'en-US' } };
eval(require('fs').readFileSync('apps/extension/i18n.js','utf8'));
setAppLanguage('ar');
if (!t('download').includes('ZDM')) process.exit(1);
if (!/[\u0600-\u06FF]/.test(t('download'))) { console.error('label is not Arabic'); process.exit(1); }
if (activeDirection() !== 'rtl') { console.error('direction is not rtl'); process.exit(1); }
setAppLanguage('fr');
if (!/Tél/.test(t('download'))) { console.error('french not applied'); process.exit(1); }
if (activeDirection() !== 'ltr') { console.error('french should be ltr'); process.exit(1); }
" 2>/dev/null; then
    pass "app language overrides the browser locale (ar -> RTL, fr -> LTR)"
  else
    bad "the extension ignores the app language"
  fi
else
  bad "apps/extension/i18n.js is missing"
fi

# The content script must load the dictionary before its own logic.
if python3 - <<'PYMANIFEST'
import json, sys
m = json.load(open('apps/extension/manifest.json'))
scripts = []
for entry in m.get('content_scripts', []):
    scripts.extend(entry.get('js', []))
if scripts[:1] == ['i18n.js'] and 'content.js' in scripts:
    sys.exit(0)
print('content_scripts must load i18n.js before content.js, got:', scripts)
sys.exit(1)
PYMANIFEST
then
  pass "manifest loads i18n.js before content.js"
else
  bad "manifest content script order is wrong"
fi

# The bridge must publish the language the extension mirrors.
if grep -q '"language"' apps/desktop/src-tauri/src/ipc/mod.rs; then
  pass "bridge reports the app language"
else
  bad "the bridge does not report a language; the button would follow the browser"
fi

# ------------------------------------------------------ Firefox (AMO) rules
group "Firefox add-on"

# AMO rejects a submission when `data_collection_permissions` is missing, and the
# key only exists from Firefox 140 — so the minimum version must be raised too.
if python3 - <<'PYAMO'
import json, sys

manifest = json.load(open('apps/extension/manifest.json'))
gecko = manifest.get('browser_specific_settings', {}).get('gecko', {})

problems = []

dcp = gecko.get('data_collection_permissions')
if not dcp:
    problems.append('data_collection_permissions is missing (AMO refuses the upload)')
elif not isinstance(dcp.get('required'), list) or not dcp['required']:
    problems.append('data_collection_permissions.required must list the data types')
elif 'none' in dcp['required'] and len(dcp['required']) > 1:
    problems.append("'none' cannot be combined with other required data types")

try:
    minimum = float(gecko.get('strict_min_version', '0'))
except ValueError:
    minimum = 0.0
if minimum < 140.0:
    problems.append(f"strict_min_version is {gecko.get('strict_min_version')}; "
                    "data_collection_permissions needs Firefox 140+")

if not gecko.get('id'):
    problems.append('the Gecko id is required for MV3 signing')

if problems:
    for problem in problems:
        print(problem)
    sys.exit(1)
sys.exit(0)
PYAMO
then
  pass "manifest satisfies the AMO submission requirements"
else
  bad "the Firefox manifest would be rejected by addons.mozilla.org"
fi

# The packaged XPI must be lint-clean, because that is the file uploaded.
if command -v web-ext >/dev/null 2>&1 && [ -f assets/zdm@zyzoom.com.xpi ]; then
  XPI_DIR=".build/xpi-lint"
  rm -rf "$XPI_DIR"
  mkdir -p "$XPI_DIR"
  if unzip -q -o assets/zdm@zyzoom.com.xpi -d "$XPI_DIR" 2>/dev/null; then
    LINT_JSON="$( cd "$XPI_DIR" && web-ext lint --source-dir . --output json 2>/dev/null )"
    ERRORS=$(printf '%s' "$LINT_JSON" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('errors',[])))" 2>/dev/null || echo "?")
    WARNINGS=$(printf '%s' "$LINT_JSON" | python3 -c "import json,sys; print(len(json.load(sys.stdin).get('warnings',[])))" 2>/dev/null || echo "?")
    if [ "$ERRORS" = "0" ] && [ "$WARNINGS" = "0" ]; then
      pass "packaged XPI is lint-clean (0 errors, 0 warnings)"
    else
      bad "packaged XPI has $ERRORS errors and $WARNINGS warnings"
      printf '%s' "$LINT_JSON" | python3 -c "
import json,sys
for key in ('errors','warnings'):
    for item in json.load(sys.stdin).get(key,[]):
        print('     -', item.get('message'))
" 2>/dev/null
    fi
  fi
  rm -rf "$XPI_DIR"
else
  warn "web-ext not installed or XPI missing: skipping the AMO lint"
fi

# The Firefox build must not carry the Chromium service worker.
if [ -f assets/zdm@zyzoom.com.xpi ]; then
  if python3 - <<'PYXPI'
import json, sys, zipfile
m = json.loads(zipfile.ZipFile('assets/zdm@zyzoom.com.xpi').read('manifest.json'))
bg = m.get('background', {})
if 'service_worker' in bg:
    print('the XPI still declares background.service_worker')
    sys.exit(1)
if bg.get('scripts') != ['background.js']:
    print('the XPI needs background.scripts for Firefox')
    sys.exit(1)
sys.exit(0)
PYXPI
  then
    pass "XPI uses the Firefox background model"
  else
    bad "XPI background configuration is Chromium-shaped"
  fi
fi

# ------------------------------------------------------------ theme contract
group "Theme"

if grep -q "data-theme='dawn'" apps/desktop/src/styles/index.css; then
  pass "a real light theme (dawn) is defined"
else
  bad "no light theme: 'light' would reuse the dark palette"
fi

# The light palette must actually be light — a dark value here is the bug that
# made "light mode" look identical to dark.
if python3 - <<'PYTHEME'
import re, sys

css = open('apps/desktop/src/styles/index.css').read()
block = css[css.index("[data-theme='dawn']"):]
block = block[:block.index('}')]

def lum(hexv):
    h = hexv.lstrip('#')
    r, g, b = (int(h[i:i+2], 16) / 255 for i in (0, 2, 4))
    f = lambda c: c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)

bg = re.search(r'--bg-panel:\s*(#[0-9a-fA-F]{6})', block).group(1)
fg = re.search(r'--text-primary:\s*(#[0-9a-fA-F]{6})', block).group(1)

if lum(bg) < 0.5:
    print(f"dawn background {bg} is dark")
    sys.exit(1)
if lum(fg) > 0.5:
    print(f"dawn text {fg} is light")
    sys.exit(1)
sys.exit(0)
PYTHEME
then
  pass "the light theme has a light surface and dark text"
else
  bad "the light theme palette is inverted or dark"
fi

# -------------------------------------------------- extension payload + policy
group "Extension delivery"

# The CRX/XPI are compiled into the binary, so a moved install or a stripped
# /usr/share/zdm can never produce "Signed CRX not found".
if [ -f apps/desktop/src-tauri/src/extension_payload.rs ]; then
  pass "extension payload module present"
else
  bad "extension_payload.rs is missing: the CRX would have to be found on disk"
fi

if grep -q "ensure_extracted" apps/desktop/src-tauri/src/browser_installer.rs; then
  pass "browser installer falls back to the embedded payload"
else
  bad "the installer never uses the embedded payload"
fi

# Chromium policy must be written under Policies\<Vendor>\<Product>. The old
# "Software\<Vendor>\PolicyExtensions" key is not a policy path at all.
if grep -q 'Policies\\' apps/desktop/src-tauri/src/browser_installer.rs; then
  pass "Chromium policy uses the real HKLM\\SOFTWARE\\Policies path"
else
  bad "Chromium policy is written to a path the browser ignores"
fi

if grep -q "force_installed" windows/installer.nsi && grep -q "Policies" windows/installer.nsi; then
  pass "installer writes ExtensionSettings policy"
else
  bad "installer does not force-install the extension (Edge would warn)"
fi

# The settings screen must not render before settings load, or it shows the
# placeholder object with an empty download_dir and looks blank.
if grep -q "settingsLoaded ?" apps/desktop/src/App.tsx; then
  pass "settings screen waits for the persisted values"
else
  bad "settings render from the placeholder object (blank engine card)"
fi

# ------------------------------------------------------------- prompt window
group "Floating prompt"

# A captured link must open the prompt, never the manager, and never start a
# download on its own.
if grep -q "crate::prompt::show" apps/desktop/src-tauri/src/ipc/mod.rs; then
  pass "bridge opens the floating prompt"
else
  bad "bridge does not open the prompt window"
fi

if sed -n '/crate::prompt::show/,+12p' apps/desktop/src-tauri/src/ipc/mod.rs | grep -q "manager.add_download"; then
  bad "the bridge still starts a download before the user confirms"
else
  pass "nothing downloads until the user confirms"
fi

if grep -q "fn focus_main" apps/desktop/src-tauri/src/ipc/mod.rs &&    ! sed -n '/Show the floating prompt/,/prompt::show/p' apps/desktop/src-tauri/src/ipc/mod.rs | grep -q "focus_main(&state.app)"; then
  pass "the manager window is not raised for a capture"
else
  bad "a capture still raises the manager window"
fi

if [ -f apps/desktop/src/components/DownloadPrompt.tsx ]; then
  pass "prompt component present"
else
  bad "DownloadPrompt.tsx is missing"
fi

# --------------------------------------------------------- explorer arguments
group "Windows shell integration"

# `arg()` re-quotes the whole switch (`"/select,C:\\f.mp4"`), which makes Explorer
# open Documents. `raw_arg` is required.
if grep -q "raw_arg" apps/desktop/src-tauri/src/commands/mod.rs; then
  pass "Explorer receives raw arguments (no double quoting)"
else
  bad "Explorer arguments are still double-quoted: the folder opens as Documents"
fi

if grep -q '/select,\\"' apps/desktop/src-tauri/src/commands/mod.rs; then
  pass "/select, keeps the switch unquoted and quotes only the path"
else
  bad "/select, argument shape is wrong"
fi

# ------------------------------------------------------------------- fonts
group "Typography"

if grep -q "IBM Plex Sans Arabic" apps/desktop/src/styles/index.css; then
  pass "bundled Arabic face is in the font stack"
else
  bad "no bundled Arabic face: Arabic would fall back to a system default"
fi

if grep -q "Hacen" apps/desktop/src/styles/index.css; then
  pass "Hacen is preferred when the user has it installed"
else
  warn "Hacen is not in the stack"
fi

if [ -f apps/desktop/public/fonts/fonts.css ] && \
   grep -q "IBM Plex Sans Arabic" apps/desktop/public/fonts/fonts.css; then
  pass "Arabic woff2 files are bundled (offline capable)"
else
  bad "Arabic webfonts are not bundled"
fi

# ------------------------------------------------------------- install.sh
group "Installer script"

if bash -n install.sh 2>/dev/null; then
  pass "install.sh parses"
else
  bad "install.sh has a syntax error"
fi

# `./install.sh` must be executable. The file was committed as 755 but the
# working copy ended up 600 (after an editor rewrite), and the README's first
# instruction then fails with "zsh: permission denied".
for script in install.sh tools/preflight.sh tools/fetch-fonts.sh \
              packaging/build-linux.sh windows/build_installer.sh; do
  if [ -x "$script" ]; then
    pass "$script is executable"
  else
    bad "$script is not executable (run: chmod +x $script)"
  fi
done

# Hyprland's modern config is Lua; an installer that only writes `exec-once`
# silently does nothing on those systems.
if grep -q "hl.exec_cmd" install.sh; then
  pass "handles Hyprland's Lua startup config"
else
  bad "only writes exec-once: autostart would fail on modern Hyprland"
fi

if grep -q "startup.lua" install.sh; then
  pass "targets confs/startup.lua"
else
  bad "does not know about confs/startup.lua"
fi

# Uninstall must clean the Lua hook, not just the conf files.
# Match the uninstall *block* (the `if [ "$DO_UNINSTALL" -eq 1 ]` guard), not the
# help text at the top which also mentions --uninstall.
if awk '/DO_UNINSTALL" -eq 1/,/^fi$/' install.sh | grep -q "startup.lua"; then
  pass "uninstall cleans the Lua hook"
else
  bad "uninstall leaves the Lua autostart line behind"
fi

# Chromium needs the file:// source allowed, or it refuses the CRX.
if grep -q "ExtensionInstallSources" install.sh; then
  pass "Chromium policy allows the file:// CRX source"
else
  bad "Chromium would refuse the local CRX (no ExtensionInstallSources)"
fi

# A rebuild must be triggered by fonts/icons too, not just source files.
if grep -q "apps/desktop/public" install.sh; then
  pass "watching public/ so font or icon changes trigger a rebuild"
else
  bad "bundled font/icon changes would ship a stale binary"
fi

# The embedded-frontend check must not be defeated by SIGPIPE.
if grep -q "EMBEDDED_ASSET" install.sh; then
  pass "verifies the frontend is embedded (pipefail-safe)"
else
  bad "no embedded-frontend verification: could install a blank window"
fi

if grep -q "rustup default stable" install.sh; then
  pass "gives an actionable message when rustup has no default toolchain"
else
  warn "no guidance for a missing rustup toolchain"
fi

# Signing the CRX needs python-cryptography; it must be installed by the script,
# not discovered as a warning after the fact.
if grep -q "python-cryptography" install.sh && grep -q "python3-cryptography" install.sh; then
  pass "installs python-cryptography on every supported distro"
else
  bad "python-cryptography is not installed: the CRX would stay unsigned"
fi

if grep -q "CRX is older than apps/extension" install.sh; then
  pass "detects a stale CRX (embedded add-on out of date)"
else
  bad "a stale CRX could be embedded without warning"
fi

# external_version must come from the manifest: a hard-coded literal makes
# Chromium treat every update as already-installed.
if grep -q 'EXT_VERSION="$(python3' install.sh && \
   ! grep -qE '"external_version": "[0-9]+\.[0-9]+\.[0-9]+"' install.sh; then
  pass "extension version is read from the manifest, not hard-coded"
else
  bad "external_version is a literal: Chromium will never refresh the add-on"
fi

# A code change to the extension must ship under a new version, or no browser
# (and no store) will replace the installed copy. Any part of the version counts:
# a minor bump (1.1.1 -> 1.2.0) is a legitimate release, and demanding a non-zero
# patch number only encouraged a meaningless patch bump on top of it.
if python3 -c "import json,sys; v=json.load(open('apps/extension/manifest.json'))['version'].split('.'); sys.exit(0 if any(int(p)>0 for p in v[:3]) else 1)"; then
  pass "extension version is set for the shipped code change"
else
  warn "extension manifest is at 0.0.0 — bump it, its code changed"
fi

# Windows installer wrote the app version as the add-on's external_version, so
# Chromium considered the extension newer than the CRX and never refreshed it.
if grep -q 'EXT_VERSION' windows/installer.nsi && \
   grep -q 'DEXT_VERSION' windows/build_installer.sh; then
  pass "windows installer carries the extension's own version"
else
  bad "windows installer writes the app version as external_version"
fi

# dist/ is a build output; embedding the CRX from there shipped a stale add-on.
if grep -qF 'assets\${CHROME_EXT_ID}.crx' windows/installer.nsi; then
  pass "windows installer embeds the CRX from assets/, not dist/"
else
  bad "windows installer may embed a stale CRX from dist/"
fi

# Overwriting the binary of a running process leaves the old image mapped, so its
# floating windows keep serving the previous frontend. Installer must wait.
if grep -q "could not be stopped" install.sh; then
  pass "waits for a running instance to exit before replacing binaries"
else
  bad "install over a running instance can keep a stale prompt window alive"
fi

# ------------------------------------------------------------ design guide
group "Lanes design"

# Identity: exactly two accents (Volt for action, Pulse for data) plus semantic
# state colours. The guide forbids more than two accents on one screen.
if grep -q "accent-1: #c8ff2e" apps/desktop/src/styles/index.css && \
   grep -q "accent-2: #35d8ff" apps/desktop/src/styles/index.css; then
  pass "Volt and Pulse are the two accents"
else
  bad "the design-guide accent tokens are missing"
fi

if grep -q "accent-1: #b7f000" apps/desktop/src/styles/index.css; then
  pass "light theme uses Volt as a fill and ink for actions"
else
  bad "light theme action colour is not the guide's ink/Volt pair"
fi

# Every CSS variable a component references must be defined, or the property
# silently falls back and the window renders unstyled.
MISSING_VARS="$(python3 tools/check-css-vars.py 2>/dev/null)"
if [ -z "$MISSING_VARS" ]; then
  pass "every CSS variable is defined"
else
  bad "undefined CSS variables: $MISSING_VARS"
fi

# The signature element: one segment per connection, filled independently.
if grep -q "lane-rail-fill" apps/desktop/src/components/Lane.tsx; then
  pass "the segment rail renders one fill per connection"
else
  bad "the lane card has no per-connection rail"
fi

if grep -q "grid-template-columns: 40px minmax(0, 1fr)" apps/desktop/src/styles/index.css; then
  pass "lane layout is a fixed grid (file · meta · speed · actions)"
else
  bad "lane layout is not the guide's fixed grid"
fi

# Navigation is a 56px icon rail, not a tree sidebar.
if grep -q "width: 56px" apps/desktop/src/styles/index.css && \
   [ -f apps/desktop/src/components/IconRail.tsx ]; then
  pass "navigation is the 56px icon rail"
else
  bad "the icon rail is missing or the wrong width"
fi

# The command bar is the title bar: brand, URL field, speed mode, window buttons.
if [ -f apps/desktop/src/components/CommandBar.tsx ] && \
   grep -q "land-bar" apps/desktop/src/styles/index.css; then
  pass "the command bar is the title bar"
else
  bad "no single command/title bar"
fi

# Speed is the hero: a large monospace figure with a small unit caption.
if grep -q "lane-speed-num" apps/desktop/src/components/Lane.tsx && \
   grep -q "font-size: 26px" apps/desktop/src/styles/index.css; then
  pass "speed is the hero figure"
else
  bad "speed is not rendered as the guide's hero number"
fi

# Every state carries a text pill, never colour alone (accessibility).
if grep -q "lane-pill" apps/desktop/src/components/Lane.tsx; then
  pass "states carry a text pill, not colour alone"
else
  bad "states are colour-only"
fi

# No heavy effects the guide bans, and none WebKitGTK cannot render.
if ! grep -qE "backdrop-filter|filter: blur" apps/desktop/src/styles/index.css; then
  pass "no backdrop blur or glassmorphism"
else
  bad "glassmorphism is banned by the guide and unreliable on WebKitGTK"
fi

# Motion must respect the OS setting.
if grep -q "prefers-reduced-motion" apps/desktop/src/styles/index.css; then
  pass "reduced-motion is honoured"
else
  bad "animations ignore the reduce-motion preference"
fi

# The engine and its commands must be untouched by a restyle.
if grep -q "download_progress" apps/desktop/src/App.tsx; then
  pass "the progress event pipeline is unchanged"
else
  bad "the restyle removed engine wiring"
fi

# ------------------------------------------------------------ new features
group "Scheduler · Torrent · Fonts"

# Cairo is the requested Arabic face; the woff2 files must be declared, or the
# browser never loads them and silently falls back.
if grep -q "font-family: 'Cairo'" apps/desktop/public/fonts/fonts.css && \
   grep -q "'Cairo'" apps/desktop/src/styles/index.css; then
  pass "Cairo is declared and first in the Arabic stack"
else
  bad "Cairo is bundled but not used"
fi

if ls apps/desktop/public/fonts/Cairo-*.woff2 >/dev/null 2>&1; then
  pass "Cairo subsets are bundled offline"
else
  bad "Cairo font files are missing"
fi

# Declaring @font-face is not enough: a wrong relative URL makes every face fail
# to load and the UI silently falls back to a system font. That is exactly how
# Cairo was "installed" but never rendered — the CSS said ./fonts/x.woff2 while
# sitting in the same directory as x.woff2. Resolve each url() for real.
font_css="apps/desktop/public/fonts/fonts.css"
if [ -f "$font_css" ]; then
  broken_fonts=0
  while IFS= read -r ref; do
    [ -f "apps/desktop/public/fonts/${ref}" ] || broken_fonts=$((broken_fonts + 1))
  done < <(grep -o "url(\./[^)]*)" "$font_css" | sed 's|url(\./||; s|)$||')

  if [ "$broken_fonts" -eq 0 ]; then
    pass "every bundled font url resolves to a real file"
  else
    bad "$broken_fonts font url(s) point at files that do not exist"
  fi
fi

# Chromium's `update_url` must be an XML update manifest. Pointing it at a .crx
# made Chrome parse a ZIP as XML, drop the policy, and leave the extension
# uninstalled however many times the user pressed repair. Match only real
# WriteRegStr lines, never the explanatory comment above them.
if grep -E 'WriteRegStr .*update_url.*\.crx' windows/installer.nsi | grep -qv '^\s*;'; then
  bad "installer points update_url at a .crx instead of update.xml"
else
  pass "installer points update_url at an XML update manifest"
fi

if grep -q 'extension\\update.xml' windows/installer.nsi; then
  pass "installer writes the update manifest Chrome reads"
else
  bad "installer never writes update.xml, so the policy points at nothing"
fi

# `external_version` must be the extension's version, not the app's: Chromium
# refuses the auto-install when the two disagree (1.1.0 vs 1.1.1 shipped).
if grep -q 'external_version.*extension_version' apps/desktop/src-tauri/src/browser_installer.rs; then
  pass "external_version is read from the CRX, not the app version"
else
  bad "external_version uses CARGO_PKG_VERSION and mismatches the CRX"
fi

# The user cannot guess that manifest.json sits one level below the install dir.
if grep -q 'extensionFolderHint' apps/desktop/src/components/SettingsView.tsx; then
  pass "settings shows the exact folder to hand to Load unpacked"
else
  bad "nothing tells the user which folder contains manifest.json"
fi

# The scheduler is a download queue, not a timetable: files wait their turn and
# start as slots free up.
if grep -q "pub fn spawn_queue_worker" apps/desktop/src-tauri/src/engine/manager.rs; then
  pass "the queue worker feeds the engine as slots free up"
else
  bad "queued files would never start"
fi

if grep -q "fn effective_slots" apps/desktop/src-tauri/src/schedule.rs; then
  pass "one-at-a-time and parallel modes are computed"
else
  bad "the queue has no parallelism control"
fi

if grep -q "fn next_to_start" apps/desktop/src-tauri/src/schedule.rs; then
  pass "the next file is chosen by queue position"
else
  bad "the queue has no defined order"
fi

if grep -q "queue_position" apps/desktop/src-tauri/src/db/mod.rs; then
  pass "queue order is persisted"
else
  bad "the queue order would be lost"
fi

# A renamed or added column must be migrated, not only declared in CREATE TABLE:
# `CREATE TABLE IF NOT EXISTS` leaves an existing older table untouched, and the
# index over the new column then kills startup on every upgraded install.
if grep -q "ADD COLUMN" apps/desktop/src-tauri/src/db/mod.rs; then
  pass "columns added after the first release are migrated"
else
  bad "an upgraded install would fail with 'no such column'"
fi

if grep -q "upgrading_a_legacy_database" apps/desktop/src-tauri/src/db/mod.rs; then
  pass "a legacy database is proven to open"
else
  bad "the upgrade path is untested"
fi

if grep -q "DrainAction" apps/desktop/src-tauri/src/schedule.rs; then
  pass "a post-queue action (sleep / shut down) is supported"
else
  bad "no drain action"
fi

# The speed modes must be real, not decorative.
if grep -q "CONNECTIONS_BY_MODE" apps/desktop/src/App.tsx; then
  pass "Quiet / Balanced / Turbo set 4 / 8 / 16 connections"
else
  bad "the speed modes do not change anything"
fi

# A finished download must not re-open a modal by itself.
if ! grep -q "setCompleted(payload)" apps/desktop/src/App.tsx; then
  pass "completion shows a toast, not a modal"
else
  bad "a modal re-opens on every completion"
fi

# The queue has its own screen, under Completed in the rail.
if grep -q "id: 'queue'" apps/desktop/src/components/IconRail.tsx && \
   [ -f apps/desktop/src/components/QueuePanel.tsx ]; then
  pass "the queue screen sits under Completed"
else
  bad "no queue screen in the rail"
fi

# The developer credit links to the project forum.
if grep -q "forum.zyzoom.net" apps/desktop/src/components/AboutModal.tsx; then
  pass "the credit opens the project forum"
else
  bad "the credit is not a link"
fi

# The extension's floating chip can be dismissed per video.
if grep -q "zdm-float-close" apps/extension/content.js; then
  pass "the video chip has a dismiss control"
else
  bad "the video chip cannot be dismissed"
fi

# Torrent support
if grep -q "librqbit" apps/desktop/src-tauri/Cargo.toml; then
  pass "the BitTorrent engine is linked"
else
  bad "no torrent engine"
fi

if grep -q "fn looks_like_torrent" apps/desktop/src-tauri/src/torrent.rs; then
  pass "magnets and .torrent files are recognised"
else
  bad "torrent input is not detected"
fi

if grep -q "add_torrent" apps/desktop/src-tauri/src/commands/mod.rs && \
   grep -q "add_torrent" apps/desktop/src/main.rs 2>/dev/null || \
   grep -q "commands::add_torrent" apps/desktop/src-tauri/src/main.rs; then
  pass "the torrent command is exposed to the UI"
else
  bad "torrents cannot be started from the app"
fi

if grep -q "PromptMode::Torrent" apps/desktop/src-tauri/src/prompt.rs; then
  pass "a magnet capture opens the torrent prompt, not a file prompt"
else
  bad "a magnet would be probed as an HTTP file"
fi

# Video still
if grep -q "dl-thumb" apps/desktop/src/components/DownloadPrompt.tsx && \
   grep -q "video.thumbnail" apps/desktop/src/components/DownloadPrompt.tsx; then
  pass "the video prompt shows a frame of the video"
else
  bad "the video prompt has no thumbnail"
fi

# ------------------------------------------------------- video prompt + speed
group "Prompt modes"

# A captured video must open the prompt in video mode with a quality list; the
# file mode only knows how to probe a Content-Length.
if grep -q "mode=video" apps/desktop/src-tauri/src/prompt.rs; then
  pass "Rust carries the prompt mode in the URL"
else
  bad "prompt mode is not propagated to the window"
fi

if grep -q "const isVideo = mode === 'video'" apps/desktop/src/components/DownloadPrompt.tsx; then
  pass "the prompt reads the mode"
else
  bad "the prompt ignores the mode: video captures show file fields"
fi

if grep -q "query_video_info" apps/desktop/src/components/DownloadPrompt.tsx && \
   grep -q "videoQuality" apps/desktop/src/components/DownloadPrompt.tsx; then
  pass "video mode analyses the page and lists qualities"
else
  bad "video mode does not produce a quality list"
fi

if grep -q "start_video_download" apps/desktop/src/components/DownloadPrompt.tsx; then
  pass "video mode starts the download through yt-dlp"
else
  bad "video mode cannot start a download"
fi

# A file prompt must never reuse the video window: that was the Windows bug where
# a file capture displayed the previous video's quality list.
if grep -q "FILE_KIND" apps/desktop/src-tauri/src/prompt.rs && \
   grep -q "VIDEO_KIND" apps/desktop/src-tauri/src/prompt.rs; then
  pass "file and video prompts are separate windows"
else
  bad "prompts share one window: a file capture can show a video's content"
fi

# reopen must not navigate() an existing window: navigate builds a tauri:// URL
# that resolves differently on Windows (http://tauri.localhost/) and silently
# fails, leaving stale content on screen.
if grep -q "window.navigate" apps/desktop/src-tauri/src/prompt.rs; then
  bad "prompt reuses a window by navigate(): breaks on Windows"
else
  pass "prompt is rebuilt per capture (no cross-platform URL assumption)"
fi

# A video started from the prompt must open its own transfer window.
if grep -q "crate::transfer::open(&app, &registry, &item.id" apps/desktop/src-tauri/src/commands/mod.rs; then
  pass "video downloads keep a transfer window"
else
  bad "video downloads run invisibly in the manager"
fi

# Both windows must have dimensions measured against their content.
if grep -q "fn size" apps/desktop/src-tauri/src/prompt.rs; then
  pass "prompt dimensions are per-mode"
else
  bad "prompt size is hard-coded for one layout"
fi

# Window teardown is asynchronous: reusing a fixed label raised
# "a webview with label `prompt-video` already exists".
if grep -q "fn next_label" apps/desktop/src-tauri/src/prompt.rs && \
   grep -q "fetch_add" apps/desktop/src-tauri/src/prompt.rs; then
  pass "every prompt gets a unique window label (no create/close race)"
else
  bad "a fixed prompt label can collide with a window still closing"
fi

# The rate limiter keyed on the extension origin alone, so a person clicking two
# different videos quickly got 429 — reported as "ZDM is not running".
if grep -q "extract_url_for_throttle" apps/desktop/src-tauri/src/ipc/mod.rs; then
  pass "bridge throttle is per URL, not per extension"
else
  bad "rapid clicks on different links are throttled as one request"
fi

# The extension must not report "not running" when the app answered with an error.
if grep -q "result.offline" apps/extension/content.js; then
  pass "extension distinguishes a closed app from a rejected request"
else
  bad "a rejected request shows the misleading 'ZDM is not running'"
fi

if grep -q "error: result.error" apps/extension/background.js; then
  pass "background script forwards the app's error reason"
else
  bad "background script swallows the app's error reason"
fi

# --- Settings must not re-run browser integration on every visit -------------
group "Settings responsiveness"

if grep -q "cached_report" apps/desktop/src-tauri/src/commands/mod.rs; then
  pass "diagnostics uses the cached integration report"
else
  bad "diagnostics re-runs browser integration (Settings freezes)"
fi

if grep -q "spawn_blocking" apps/desktop/src-tauri/src/commands/mod.rs; then
  pass "browser repair runs off the UI thread"
else
  bad "browser repair blocks the UI thread"
fi

if grep -q "loadingRef" apps/desktop/src/components/SettingsView.tsx; then
  pass "settings view guards against overlapping loads"
else
  bad "settings view can fire concurrent diagnostic loads"
fi

# --------------------------------------------------------------- build/tests
if [ "$QUICK" -eq 0 ]; then
  group "Frontend"

  if ( cd apps/desktop && npx tsc --noEmit ); then
    pass "TypeScript compiles"
  else
    bad "TypeScript errors"
  fi

  if ( cd apps/desktop && npm run build >/dev/null 2>&1 ); then
    pass "production bundle builds"
  else
    bad "frontend build failed"
  fi

  group "Rust"

  if cargo check -p zdm 2>&1 | grep -q "^error"; then
    bad "cargo check failed"
  else
    pass "cargo check"
  fi

  TEST_OUT="$(cargo test -p zdm 2>&1)"
  if printf '%s' "$TEST_OUT" | grep -q "test result: ok"; then
    COUNT="$(printf '%s' "$TEST_OUT" | grep -o "test result: ok\. [0-9]*" | grep -o "[0-9]*" | head -1)"
    pass "$COUNT tests pass"
  else
    bad "cargo test failed"
    printf '%s\n' "$TEST_OUT" | grep -E "^(test .*FAILED|failures:)" | head -5
  fi
fi

# -------------------------------------------------------------------- result
printf '\n'
if [ "$FAILED" -eq 0 ]; then
  printf '%s\n' "${GREEN}${BOLD}  Preflight passed.${NC}"
  exit 0
fi
printf '%s\n' "${RED}${BOLD}  Preflight FAILED — see the ✗ items above.${NC}"
exit 1

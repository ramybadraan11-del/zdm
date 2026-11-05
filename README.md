<div align="center">

<img src="assets/logo.svg" alt="ZDM" width="120"/>

# Zyzoom Download Manager (ZDM)

### A multi-segment download accelerator for Linux and Windows

**Developed by Ramy Badraan**

[![Stack](https://img.shields.io/badge/engine-Tauri%202%20%C2%B7%20Rust%20%C2%B7%20Tokio-orange.svg)](#)
[![UI](https://img.shields.io/badge/ui-React%2018%20%C2%B7%20TypeScript%20%C2%B7%20Tailwind-cyan.svg)](#)
[![Languages](https://img.shields.io/badge/languages-%D8%A7%D9%84%D8%B9%D8%B1%D8%A8%D9%8A%D8%A9%20%C2%B7%20English%20%C2%B7%20Fran%C3%A7ais-green.svg)](#)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%C2%B7%20Windows-blue.svg)](https://github.com/ramybadraan11-del/zdm-releases)

</div>

---

## What it does

ZDM splits a download into parallel byte ranges and writes each chunk straight
to its final offset on disk — no temporary files, no assembly step at 99%.

- **Multi-segment transfers** — 2 to 32 parallel connections, chosen from the
  file size, with per-segment resume offsets saved to SQLite.
- **Real pause / resume / cancel** — pausing waits for the writers to stop before
  the UI says "paused"; resuming continues from the exact byte on disk.
- **Browser capture** — Chrome, Chromium, Brave, Edge, Vivaldi, Firefox and Zen
  can hand downloads to ZDM. A download is only taken over after ZDM confirms it
  accepted the request, so a closed app never swallows a file.
- **Video download** — yt-dlp integration with a quality picker, MP3 extraction
  and progress reported through the same pipeline as normal downloads.
- **Three languages** — Arabic (RTL), English and French, switchable live.
- **Tray + autostart** — works on GNOME, KDE, XFCE, and on bare Hyprland/sway/i3
  where XDG autostart alone is ignored.

---

## Install

### Linux — any distribution

```bash
./install.sh                 # install for the current user
./install.sh --system        # install system-wide (sudo)
./install.sh --deps          # runtime dependencies only
./install.sh --rebuild       # force a release rebuild first
./install.sh --uninstall     # remove everything the script created
./install.sh --dry-run       # print the plan, change nothing
```

The script detects Arch, Fedora, Debian/Ubuntu and openSUSE, installs the WebKit
and AppIndicator runtime, builds the binaries, and wires **all three** autostart
mechanisms (XDG, systemd --user, compositor `exec-once`).

### Linux — native packages

```bash
bash packaging/build-linux.sh     # builds deb + rpm + AppImage into dist/
bash packaging/verify.sh          # validates every produced package
```

| Format | Target |
|---|---|
| `.deb` | Debian, Ubuntu, Mint, Pop!\_OS, Kali, Zorin |
| `.rpm` | Fedora, RHEL, Rocky, Alma, openSUSE |
| `.AppImage` | any distribution, no root, single file |

> Since 1.3.0 the `.rpm` is GPG-signed. Import the release key once, then verify
> and install normally:
>
> ```bash
> sudo rpm --import docs/zdm-rpm-pubkey.asc
> sudo rpm -i zyzoom-download-manager-<version>-1.x86_64.rpm
> sudo rpm -K zyzoom-download-manager-<version>-1.x86_64.rpm   # digests + signatures OK
> ```
>
> The `.deb` and AppImage need no extra flags.

---

## Releases and updates

Official builds are published on the project's public releases repository.
Every release ships deb/rpm/AppImage/Windows installers plus an updater manifest
(`latest.json`). It is linked from the project's GitHub page (and from the issue
tracker); signed by the same GPG key used for RPM packages.

The Windows installer and the AppImage update themselves in place — ZDM checks the
channel and installs with one click (Settings → Updates). The deb and rpm packages
update through your package manager (`dnf upgrade`, `apt upgrade`).

> **RPM build note:** `tools/mk-rpm.py` must **not** emit explicit directory
> entries in the cpio payload — rpm creates parent directories itself, and
> explicit dir entries make Fedora fail with `cpio: Archive file not in header`.
> `packaging/verify.sh` runs a real install in a user namespace to catch this.

### Windows

Windows gets a **full installer**, not a portable folder. It installs to
`Program Files`, registers the native-messaging host and the signed CRX3, writes
one autostart entry under `HKCU`, bundles yt-dlp, and ships a clean uninstaller.

**Cross-compile from Linux** (no Windows machine needed):

```bash
cargo install cargo-xwin --locked      # once
bash windows/build_installer.sh        # builds zdm.exe + the installer
```

Output: `dist/Zyzoom_Download_Manager_Setup_<version>.exe` — **one self-contained
file, ~228 MB**, that installs everything:

| Bundled | Why |
|---|---|
| `zdm.exe`, `zdm-host.exe` | the app and the browser native host |
| signed CRX3 + Firefox XPI | extension auto-install, no manual "load unpacked" |
| enum of 3 locales | Arabic / English / French |
| `yt-dlp.exe` | video download works on first run, no download needed |
| `ffmpeg.exe` (105 MB, 37 MB compressed) | **merges** the separate video and audio streams into one file. Not optional: without it every YouTube download above 720p lands as `Title.f137.mp4` + `Title.f140.m4a` |
| two architectures | `..._x64.exe` and `..._x86.exe`; each installs only its own build |
| WebView2 bootstrapper (1.8 MB) | installs the runtime only where it is missing |

Total: **~49 MB**. Without ffmpeg, YouTube downloads above 720p land as two
files (`Title.f137.mp4` + `Title.f140.m4a`) — that is why it ships in the box.

Set `ZDM_SLIM_INSTALLER=1` to build the ~21 MB online variant instead (WebView2
is then expected on the system).

The script builds `makensis` from source on first run, since the official NSIS
binary is Windows-only and Wine is not always available.

**On Windows** use the native toolchain instead:

```powershell
powershell -ExecutionPolicy Bypass -File windows\build-installer.ps1
```

CI builds it on a Windows runner too — see `.github/workflows/build.yml`.

> **Long transfers must not use a whole-request timeout.** `reqwest`'s
> `Client::timeout` bounds the entire request, so a 45 s cap aborted any download
> slower than that — which is what cut the 110 MB ffmpeg fetch at ~2.8 MB with
> "error decoding response body". `build_client()` now bounds only the connection
> and per-read stalls; `build_probe_client()` keeps a short cap for metadata.
>
> **The extension follows the app language, not the browser's.** `chrome.i18n`
> can only report the browser locale, so the floating button reads the app's
> language from the local bridge and falls back to the browser locale only when
> the app is not running. When the language changes the button is re-mounted —
> a previous version read `activeVideo` *after* `unmountButton()` had cleared it,
> so the label silently stayed in the old language.
>
> **The extension payload is compiled into the binary.** The CRX3 and XPI are
> ~34 KB each, so `extension_payload.rs` embeds them with `include_bytes!` and
> writes them to the data directory on first run. A moved install, a stripped
> `/usr/share/zdm` or an older installer can no longer produce
> "Signed CRX not found".
>
> **Chromium policy must use the policy registry path.** Edge and Chrome read
> `HKLM\SOFTWARE\Policies\<Vendor>\<Product>\ExtensionSettings\<id>`. Writing
> to `Software\<Vendor>\PolicyExtensions` (the previous key) is silently ignored,
> which is why Edge kept asking whether to trust the add-on.
>
> **Self-signed CRX needs an enterprise policy.** Chromium treats a locally
> signed extension as an unknown source and shows "not from the Store" — on Edge
> that reads as "untrusted tool". The installer and the app both write
> `ExtensionSettings` / `Extensions\<id>` policy so it installs silently and
> pins the toolbar button.
>
> **Release builds must embed the frontend.** The `custom-protocol` feature is
> enabled by default in `src-tauri/Cargo.toml` for *both* `tauri` and
> `tauri-macros`. Without it a release binary opens `http://localhost:1420` and
> the window shows "Could not connect to localhost: Connection refused".
> `tools/preflight.sh` fails the build if that wiring regresses.

---

## The interface

Three themes (`moon`, `dawn`, `system`) drive one token contract, so switching is
a single attribute on `<html>`. Every surface, border and text colour is a token;
no component hard-codes a hex.

**Two windows, on purpose:**

- The **manager** is a list, a toolbar and settings.
- **Capturing a link never opens the manager.** A small frameless window appears
  near the pointer with the file facts and three choices — start now, start
  later, or cancel — plus the destination folder. Nothing is written until you
  press start, and the manager window stays out of the way.
- Pressing start opens a **separate transfer window** per download, with one
  ribbon column per connection filling bottom-up. Closing it pauses nothing: the
  transfer keeps running.

Live windows are keyed `transfer-<id>`, so reopening focuses the existing one
instead of spawning duplicates, and the tray exposes "show transfer windows".

## Typography

Arabic uses **IBM Plex Sans Arabic** (SIL OFL, bundled as woff2 — works offline).
Hacen Tunisia is listed first in the stack, so it is used automatically when the
user already has it installed; it is free for personal use only and therefore not
redistributed with the app. Latin falls through to Inter, which is bundled too.

## Autostart on Hyprland

Hyprland has shipped two configuration styles, and an installer that only knows
one silently does nothing:

| Layout | What ZDM writes |
|---|---|
| classic `hyprland.conf` | `exec-once = '<path>' --background` |
| modern split Lua (`confs/startup.lua`) | `hl.exec_cmd("'<path>' --background")` inserted **inside** the existing `hl.on("hyprland.start", …)` handler |

XDG autostart and a `systemd --user` unit are written as well, so the app starts
in the background on GNOME, KDE, XFCE, sway and i3 too. The file is only ever
appended to, and `tools/preflight.sh` verifies the Lua stays parseable.

## How the engine stays correct

Download corruption is easy to write and hard to notice, so the engine is built
around a few invariants that are covered by tests:

| Invariant | Why it matters |
|---|---|
| A `200 OK` answer to a `Range:` request is a protocol downgrade, never written at an offset | Writing a full body at a non-zero offset is the classic corruption bug |
| Every resumed range sends `If-Range` with the stored ETag | Otherwise a file replaced upstream gets spliced onto an old prefix |
| Completion is proven by summing written bytes, not by file length | The file is preallocated with `set_len`, so length proves nothing |
| HTTP `416` is a hard error | It means the remote file changed; treating it as "done" leaves a zero-filled tail |
| One async lock per download id serialises start/pause/resume/delete | Two writers on the same ranges is the other classic corruption bug |
| An unknown-length body is one open-ended stream encoded as `(0, 0)` | `u64::MAX` cannot survive a round-trip through SQLite's signed integers |

Run the tests:

```bash
cargo test -p zdm
```

The suite includes end-to-end runs against local HTTP servers that serve ranges,
ignore ranges, omit `Content-Length`, and trickle bytes slowly enough to pause
mid-transfer.

---

## Tray icon on Hyprland / sway

Wayland compositors have no tray of their own. ZDM registers a StatusNotifier
item; your bar has to display it:

```json
"modules-right": ["pulseaudio", "network", "tray", "clock"],
"tray": { "icon-size": 18, "spacing": 10 }
```

If no tray host is running, the app still works — Settings → Diagnostics says so
explicitly instead of leaving you wondering where the icon went. GNOME needs the
"AppIndicator and KStatusNotifierItem Support" extension.

---

## Project layout

```
apps/desktop/          Tauri app (Rust engine + React UI)
  src-tauri/src/
    engine/            downloader (probing), manager (scheduling, writers)
    ipc/               local HTTP bridge for the browser extension
    browser_installer/ native-messaging + CRX registration
    autostart.rs       XDG + systemd + compositor wiring
    tray.rs, i18n.rs, paths.rs, process.rs
  src/                 React: components, i18n dictionary, types
apps/extension/        MV3 extension (Chromium + Firefox), 3 locales
apps/host/             native-messaging host (stdin/stdout → bridge)
packaging/             deb/rpm/AppImage scripts, spec, verification
windows/               NSIS installer + PowerShell build
tools/                 icon factory, CRX3 signer, RPM writer, font fetcher
```

---

## Known limits

- **Firefox (non-ESR) only installs Mozilla-signed add-ons.** The installer
  writes an enterprise policy that force-installs the bundled XPI; if the build
  refuses it, install the XPI once through `about:addons`. This is a Mozilla
  policy, not a ZDM defect.
- **The Windows `.exe` is cross-compiled from Linux** with `cargo-xwin`, so the
  MSVC toolchain is downloaded automatically and NSIS is built from source. The
  Windows path (`build-installer.ps1`) is still the recommended one when you
  have a Windows machine, because it uses the native toolchain.
- **ffmpeg is required for merged video.** ZDM bundles it on Windows and looks
  for it everywhere on Linux (the installer adds the distro package). If it is
  missing at runtime the app fetches a static build once into its data folder.
- **Speed depends on the server.** A host that throttles per connection will not
  get faster with more segments; one that refuses ranges gets a single stream.

---

## Licence

Proprietary. Copyright © 2026 Ramy Badraan. All rights reserved.
Third-party components keep their own licences — see `packaging/LICENSE.txt`.

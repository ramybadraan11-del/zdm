; ==============================================================================
; Zyzoom Download Manager (ZDM) — Windows Installer (NSIS 3)
; Developed by Ramy Badraan
;
; This is a FULL installer, not a portable copy. It installs to Program Files,
; registers the app, the native-messaging host, the browser extension and the
; autostart entry, and ships every runtime asset the app needs.
;
; Build (on Windows, from the repo root):
;   makensis windows\installer.nsi
;
; What it fixes compared with the previous script:
;   • ONE autostart entry (HKCU only). Writing both HKLM and HKCU Run values is
;     what launched the app twice at login and produced two tray icons.
;   • Native-messaging manifests are generated with ABSOLUTE paths and a single
;     exact `allowed_origins` value; a relative path or a wildcard makes Chrome
;     reject the whole file, which is why capture never worked on Windows.
;   • The extension is installed through the supported `external_crx` channel
;     with a signed CRX3, so Chromium can auto-install it.
;   • WebView2 is detected and installed when missing (Tauri needs it).
;   • yt-dlp ships with the installer so video download works offline at first
;     run, and the app never has to fetch it from a console window.
; ==============================================================================

Unicode true
SetCompressor /SOLID lzma
SetCompressorDictSize 64

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

; ------------------------------------------------------------------ metadata
!define PRODUCT_NAME        "Zyzoom Download Manager"
!define PRODUCT_SHORT       "ZDM"
!define PRODUCT_VERSION     "1.3.0"

; The add-on has its own version. Chromium compares external_version against the
; CRX it already has, so writing the *app* version here made the extension look
; older than it is and it was never refreshed. build_installer.sh overwrites this
; with the value from apps/extension/manifest.json.
!ifndef EXT_VERSION
  !define EXT_VERSION "1.4.0"
!endif
!define PRODUCT_PUBLISHER   "Ramy Badraan"
!define PRODUCT_URL         "https://zyzoom.net"
!define PRODUCT_EXE         "zdm.exe"
!define PRODUCT_HOST_EXE    "zdm-host.exe"
!define REG_UNINSTALL       "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZyzoomDownloadManager"
!define REG_RUN             "Software\Microsoft\Windows\CurrentVersion\Run"
!define REG_APP_PATH        "Software\Microsoft\Windows\CurrentVersion\App Paths\zdm.exe"
!define NATIVE_HOST_NAME    "com.zyzoom.zdm"
!define CHROME_EXT_ID       "jgoccphfikadefpdemmaohkpbomgjcak"
!define GECKO_EXT_ID        "zdm@zyzoom.com"

!define REPO_ROOT           ".."
; Where zdm.exe and zdm-host.exe live.
; On Windows a plain `cargo build --release` writes to target\release.
; A cross-build from Linux (cargo xwin) writes to the target-triple folder.
; Override from the command line with /DWIN_RELEASE_DIR=...
; Target architecture of this build: x64 (default) or x86.
!ifndef WIN_ARCH
  !define WIN_ARCH "x64"
!endif

!ifndef WIN_RELEASE_DIR
  !ifdef NSIS_PLATFORM_POSIX
    !if "${WIN_ARCH}" == "x86"
      !define WIN_RELEASE_DIR "target/i686-pc-windows-msvc/release"
    !else
      !define WIN_RELEASE_DIR "target/x86_64-pc-windows-msvc/release"
    !endif
  !else
    !define WIN_RELEASE_DIR "${REPO_ROOT}\target\release"
  !endif
!endif
!define RELEASE_DIR         "${WIN_RELEASE_DIR}"

!if "${WIN_ARCH}" == "x86"
  Name "${PRODUCT_NAME} ${PRODUCT_VERSION} (32-bit)"
!else
  Name "${PRODUCT_NAME} ${PRODUCT_VERSION} (64-bit)"
!endif
!if "${WIN_ARCH}" == "x86"
  OutFile "${REPO_ROOT}\dist\Zyzoom_Download_Manager_Setup_${PRODUCT_VERSION}_x86.exe"
!else
  OutFile "${REPO_ROOT}\dist\Zyzoom_Download_Manager_Setup_${PRODUCT_VERSION}_x64.exe"
!endif
; Pick the right Program Files for the architecture this installer was built for.
; $PROGRAMFILES64 is empty on a 32-bit-only Windows, which would install to C:\.
!if "${WIN_ARCH}" == "x86"
  InstallDir "$PROGRAMFILES32\${PRODUCT_NAME}"
!else
  InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
!endif
InstallDirRegKey HKLM "Software\${PRODUCT_SHORT}" "InstallDir"
RequestExecutionLevel admin
BrandingText "${PRODUCT_NAME} · ${PRODUCT_PUBLISHER}"

VIProductVersion "${PRODUCT_VERSION}.0"
VIAddVersionKey "ProductName"     "${PRODUCT_NAME}"
VIAddVersionKey "CompanyName"     "${PRODUCT_PUBLISHER}"
VIAddVersionKey "FileDescription" "${PRODUCT_NAME} Setup"
VIAddVersionKey "FileVersion"     "${PRODUCT_VERSION}.0"
VIAddVersionKey "ProductVersion"  "${PRODUCT_VERSION}.0"
VIAddVersionKey "LegalCopyright"  "© 2026 ${PRODUCT_PUBLISHER}"

; --------------------------------------------------------------------- pages
!define MUI_ICON   "icons\icon.ico"
!define MUI_UNICON "icons\icon.ico"
!define MUI_ABORTWARNING
; Component descriptions on the components page.
!define MUI_FINISHPAGE_NOAUTOCLOSE
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_BITMAP      "icons\header.bmp"
!define MUI_HEADERIMAGE_UNBITMAP    "icons\header.bmp"
!define MUI_WELCOMEFINISHPAGE_BITMAP "icons\wizard.bmp"
!define MUI_UNWELCOMEFINISHPAGE_BITMAP "icons\wizard.bmp"

!define MUI_WELCOMEPAGE_TITLE "Install ${PRODUCT_NAME} ${PRODUCT_VERSION}"
!define MUI_WELCOMEPAGE_TEXT  "This wizard will install ${PRODUCT_NAME} on your computer.$\r$\n$\r$\nIt includes the download engine, the browser integration for Chrome, Edge, Brave, Vivaldi, Firefox and Zen, and the video downloader.$\r$\n$\r$\nClick Next to continue."

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "license.txt"
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\${PRODUCT_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Start ${PRODUCT_NAME}"
!define MUI_FINISHPAGE_LINK "Visit the project page"
!define MUI_FINISHPAGE_LINK_LOCATION "${PRODUCT_URL}"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Arabic"
!insertmacro MUI_LANGUAGE "French"
!insertmacro MUI_LANGUAGE "SimpChinese"

; =========================================================== helper macros
; Chromium's supported auto-install channel: external_crx + external_version.
!macro RegisterExternalExtension DIR
  ${If} ${FileExists} "$INSTDIR\extension\${CHROME_EXT_ID}.crx"
    CreateDirectory "${DIR}\External Extensions"
    FileOpen $0 "${DIR}\External Extensions\${CHROME_EXT_ID}.json" w
    FileWrite $0 '{$\r$\n'
    FileWrite $0 '  "external_crx": "$INSTDIR\extension\${CHROME_EXT_ID}.crx",$\r$\n'
    FileWrite $0 '  "external_version": "${EXT_VERSION}"$\r$\n'
    FileWrite $0 '}$\r$\n'
    FileClose $0
  ${EndIf}
!macroend

; ================================================================ sections
Section "!Core application" SEC_CORE
  SectionIn RO
  SetShellVarContext all
  SetOutPath "$INSTDIR"
  SetOverwrite on

  ; Stop a running copy so files can be replaced. /T kills the tree; a running
  ; app would otherwise leave a locked exe behind.
  DetailPrint "Closing any running copy of ${PRODUCT_SHORT}…"
  nsExec::Exec 'taskkill /F /IM ${PRODUCT_EXE} /T'
  nsExec::Exec 'taskkill /F /IM ${PRODUCT_HOST_EXE} /T'
  Sleep 600

  DetailPrint "Installing the application files…"
  File "${RELEASE_DIR}\${PRODUCT_EXE}"
  File "${RELEASE_DIR}\${PRODUCT_HOST_EXE}"

  SetOutPath "$INSTDIR\icons"
  File "icons\icon.ico"

  ; Extension payload: unpacked folder (for Load unpacked) + signed CRX (auto).
  ;
  ; Read from assets/, which build-packages.py signs. dist/ is a build output: it
  ; can hold a CRX from an earlier run, and embedding that shipped a stale add-on.
  SetOutPath "$INSTDIR\extension"
  File /r "${REPO_ROOT}\apps\extension\*.*"
  ${If} ${FileExists} "${REPO_ROOT}\assets\${CHROME_EXT_ID}.crx"
    File "${REPO_ROOT}\assets\${CHROME_EXT_ID}.crx"
  ${EndIf}
  ${If} ${FileExists} "${REPO_ROOT}\assets\${GECKO_EXT_ID}.xpi"
    File "${REPO_ROOT}\assets\${GECKO_EXT_ID}.xpi"
  ${EndIf}

  ; The XML update manifest the ExtensionSettings policy points `update_url` at.
  ; Without this file Chromium has a policy referencing a missing path and keeps
  ; the extension uninstalled. CRLF so the file opens correctly on Windows.
  ${If} ${FileExists} "$INSTDIR\extension\${CHROME_EXT_ID}.crx"
    FileOpen $0 "$INSTDIR\extension\update.xml" w
    FileWrite $0 '<?xml version="1.0" encoding="UTF-8"?>$\r$\n'
    FileWrite $0 '<gupdate xmlns="http://www.google.com/update2/response" protocol="2.0">$\r$\n'
    FileWrite $0 '  <app appid="${CHROME_EXT_ID}">$\r$\n'
    FileWrite $0 '    <updatecheck codebase="file:///$INSTDIR/extension/${CHROME_EXT_ID}.crx" version="${EXT_VERSION}" />$\r$\n'
    FileWrite $0 '  </app>$\r$\n'
    FileWrite $0 '</gupdate>$\r$\n'
    FileClose $0
  ${EndIf}

  ; Bundled yt-dlp + ffmpeg so video download works on first run.
  ;
  ; ffmpeg is not optional: YouTube serves video and audio as separate streams
  ; above 720p, and without ffmpeg yt-dlp cannot merge them. It writes
  ; "<name>.f137.mp4" + "<name>.f140.m4a" instead — the "two files, path does not
  ; exist" bug. Both tools live in $INSTDIR\bin, which the app searches first.
  ${If} ${FileExists} "${REPO_ROOT}\dist\yt-dlp.exe"
    SetOutPath "$INSTDIR\bin"
    File "${REPO_ROOT}\dist\yt-dlp.exe"
  ${EndIf}
  ${If} ${FileExists} "${REPO_ROOT}\dist\ffmpeg.exe"
    SetOutPath "$INSTDIR\bin"
    File "${REPO_ROOT}\dist\ffmpeg.exe"
  ${EndIf}

  SetOutPath "$INSTDIR"

  ; Documents / licence
  WriteRegStr HKLM "Software\${PRODUCT_SHORT}" "InstallDir" "$INSTDIR"
  WriteRegStr HKLM "Software\${PRODUCT_SHORT}" "Version" "${PRODUCT_VERSION}"

  ; App Paths lets Win+R and the Start menu resolve "zdm".
  WriteRegStr HKLM "${REG_APP_PATH}" "" "$INSTDIR\${PRODUCT_EXE}"
  WriteRegStr HKLM "${REG_APP_PATH}" "Path" "$INSTDIR"

  ; Autostart: HKCU only. HKLM+HKCU is what started ZDM twice and produced two
  ; tray icons; the app also refuses a second instance now.
  WriteRegStr HKCU "${REG_RUN}" "ZyzoomDownloadManager" '"$INSTDIR\${PRODUCT_EXE}" --background'
SectionEnd

Section "Browser integration" SEC_BROWSERS
  DetailPrint "Registering the native messaging host and the extension…"

  ; --- native messaging host (per-user, which is where browsers look first)
  ; Chromium on Windows reads the host path from the registry, not from a file.
  ; On a 64-bit host we must write to the 64-bit view so Chrome/Edge (64-bit)
  ; read the native-messaging keys. On 32-bit Windows there is only one view.
  ${If} ${RunningX64}
    ${If} "${WIN_ARCH}" == "x64"
      SetRegView 64
    ${Else}
      SetRegView 32
    ${EndIf}
  ${EndIf}

  WriteRegStr HKCU "Software\Google\Chrome\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKCU "Software\Microsoft\Edge\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\edge.json"
  WriteRegStr HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKCU "Software\Vivaldi\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKCU "Software\Chromium\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKCU "Software\Mozilla\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\firefox.json"

  ; Same keys for all users (the installer is elevated, so HKLM is allowed).
  WriteRegStr HKLM "Software\Google\Chrome\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKLM "Software\Microsoft\Edge\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\edge.json"
  WriteRegStr HKLM "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKLM "Software\Mozilla\NativeMessagingHosts\${NATIVE_HOST_NAME}" "" "$INSTDIR\manifests\firefox.json"

  ; --- manifest files with absolute paths
  SetOutPath "$INSTDIR\manifests"

  ; Chrome / Chromium family
  FileOpen $0 "$INSTDIR\manifests\chrome.json" w
  FileWrite $0 '{$\r$\n'
  FileWrite $0 '  "name": "${NATIVE_HOST_NAME}",$\r$\n'
  FileWrite $0 '  "description": "Zyzoom Download Manager native host",$\r$\n'
  FileWrite $0 '  "path": "$INSTDIR\${PRODUCT_HOST_EXE}",$\r$\n'
  FileWrite $0 '  "type": "stdio",$\r$\n'
  FileWrite $0 '  "allowed_origins": ["chrome-extension://${CHROME_EXT_ID}/"]$\r$\n'
  FileWrite $0 '}$\r$\n'
  FileClose $0
  FileOpen $0 "$INSTDIR\manifests\edge.json" w
  FileWrite $0 '{$\r$\n'
  FileWrite $0 '  "name": "${NATIVE_HOST_NAME}",$\r$\n'
  FileWrite $0 '  "description": "Zyzoom Download Manager native host",$\r$\n'
  FileWrite $0 '  "path": "$INSTDIR\${PRODUCT_HOST_EXE}",$\r$\n'
  FileWrite $0 '  "type": "stdio",$\r$\n'
  FileWrite $0 '  "allowed_origins": ["chrome-extension://${CHROME_EXT_ID}/"]$\r$\n'
  FileWrite $0 '}$\r$\n'
  FileClose $0

  ; Firefox / Zen / LibreWolf
  FileOpen $0 "$INSTDIR\manifests\firefox.json" w
  FileWrite $0 '{$\r$\n'
  FileWrite $0 '  "name": "${NATIVE_HOST_NAME}",$\r$\n'
  FileWrite $0 '  "description": "Zyzoom Download Manager native host",$\r$\n'
  FileWrite $0 '  "path": "$INSTDIR\${PRODUCT_HOST_EXE}",$\r$\n'
  FileWrite $0 '  "type": "stdio",$\r$\n'
  FileWrite $0 '  "allowed_extensions": ["${GECKO_EXT_ID}"]$\r$\n'
  FileWrite $0 '}$\r$\n'
  FileClose $0

  ; --- per-profile external extension registration (supports profiles too)
  SetShellVarContext current
  !insertmacro RegisterExternalExtension "$LOCALAPPDATA\Google\Chrome\User Data"
  !insertmacro RegisterExternalExtension "$LOCALAPPDATA\Microsoft\Edge\User Data"
  !insertmacro RegisterExternalExtension "$LOCALAPPDATA\BraveSoftware\Brave-Browser\User Data"
  !insertmacro RegisterExternalExtension "$LOCALAPPDATA\Vivaldi\User Data"
  !insertmacro RegisterExternalExtension "$LOCALAPPDATA\Chromium\User Data"

  ; Multiple Chrome profiles live in "Profile 1", "Profile 2"… subfolders.
  ClearErrors
  FindFirst $1 $2 "$LOCALAPPDATA\Google\Chrome\User Data\Profile *"
  ${DoWhile} $2 != ""
    !insertmacro RegisterExternalExtension "$LOCALAPPDATA\Google\Chrome\User Data\$2"
    FindNext $1 $2
  ${Loop}
  FindClose $1

  ; --------------------------------------------------
  ; Enterprise policy: removes the "extension is not from the Store" prompt.
  ;
  ; A self-signed CRX is treated as an unknown source by Chromium. With
  ; ExtensionSettings.force_installed the browser installs it silently and pins
  ; the toolbar button, which is what users expect from a download manager.
  ; Written for Chrome, Edge, Brave and Vivaldi, per-user (HKCU) and machine-wide.
  ; --------------------------------------------------
  DetailPrint "Configuring the extension policy (no untrusted-source prompt)…"

  Call WriteChromePolicy

  ; Firefox/Zen enterprise policy: the only way to force-install without AMO.
  CreateDirectory "$APPDATA\Mozilla\Firefox\distribution"
  ${If} ${FileExists} "$INSTDIR\extension\${GECKO_EXT_ID}.xpi"
    FileOpen $0 "$APPDATA\Mozilla\Firefox\distribution\policies.json" w
    FileWrite $0 '{$\r$\n'
    FileWrite $0 '  "policies": {$\r$\n'
    FileWrite $0 '    "ExtensionSettings": {$\r$\n'
    FileWrite $0 '      "${GECKO_EXT_ID}": {$\r$\n'
    FileWrite $0 '        "installation_mode": "force_installed",$\r$\n'
    FileWrite $0 '        "install_url": "file:///$INSTDIR/extension/${GECKO_EXT_ID}.xpi"$\r$\n'
    FileWrite $0 '      }$\r$\n'
    FileWrite $0 '    }$\r$\n'
    FileWrite $0 '  }$\r$\n'
    FileWrite $0 '}$\r$\n'
    FileClose $0
  ${EndIf}

  SetShellVarContext all
SectionEnd

; Torrent file association.
;
; Without this a double-clicked .torrent opens whatever editor the machine has
; (or nothing), because no handler was ever registered. ZDM receives the path as
; argv[1] and adds it straight to the BitTorrent engine.
Section "Torrent file association" SEC_TORRENT
  ; .torrent -> ZDM.Torrent, per-user so no elevation is required.
  WriteRegStr HKCU "Software\Classes\.torrent" "" "ZDM.Torrent"
  WriteRegStr HKCU "Software\Classes\ZDM.Torrent" "" "BitTorrent file"
  WriteRegStr HKCU "Software\Classes\ZDM.Torrent\DefaultIcon" "" "$INSTDIR\${PRODUCT_EXE},0"
  WriteRegStr HKCU "Software\Classes\ZDM.Torrent\shell\open" "" "Open with ${PRODUCT_SHORT}"
  WriteRegStr HKCU "Software\Classes\ZDM.Torrent\shell\open\command" "" '"$INSTDIR\${PRODUCT_EXE}" "%1"'

  ; Machine-wide entry, so the association also shows for other users.
  WriteRegStr HKLM "Software\Classes\.torrent" "" "ZDM.Torrent"
  WriteRegStr HKLM "Software\Classes\ZDM.Torrent" "" "BitTorrent file"
  WriteRegStr HKLM "Software\Classes\ZDM.Torrent\DefaultIcon" "" "$INSTDIR\${PRODUCT_EXE},0"
  WriteRegStr HKLM "Software\Classes\ZDM.Torrent\shell\open\command" "" '"$INSTDIR\${PRODUCT_EXE}" "%1"'

  ; magnet: is a URL scheme, not a file extension. Windows resolves it through
  ; HKCR\magnet, so without this key a magnet link never reaches ZDM at all —
  ; the browser reports "no application is associated". `"URL Protocol"` is what
  ; marks the key as a protocol handler.
  WriteRegStr HKCU "Software\Classes\magnet" "" "URL:Magnet link"
  WriteRegStr HKCU "Software\Classes\magnet" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\magnet\DefaultIcon" "" "$INSTDIR\${PRODUCT_EXE},0"
  WriteRegStr HKCU "Software\Classes\magnet\shell\open\command" "" '"$INSTDIR\${PRODUCT_EXE}" "%1"'

  WriteRegStr HKLM "Software\Classes\magnet" "" "URL:Magnet link"
  WriteRegStr HKLM "Software\Classes\magnet" "URL Protocol" ""
  WriteRegStr HKLM "Software\Classes\magnet\DefaultIcon" "" "$INSTDIR\${PRODUCT_EXE},0"
  WriteRegStr HKLM "Software\Classes\magnet\shell\open\command" "" '"$INSTDIR\${PRODUCT_EXE}" "%1"'

  ; Ask Explorer to refresh its icon/association cache.
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
SectionEnd

Section "WebView2 runtime" SEC_WEBVIEW
  ; Tauri renders with Edge WebView2. Windows 11 always has it; Windows 10 may
  ; not, and without it ZDM shows a blank window.
  ReadRegStr $0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${If} $0 == ""
    ReadRegStr $0 HKCU "Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" "pv"
  ${EndIf}

  ${If} $0 == ""
    DetailPrint "Installing the WebView2 runtime…"
    ; Bundled when the build machine had it; otherwise we can only inform the user.
    !if /FILEEXISTS "${REPO_ROOT}\dist\MicrosoftEdgeWebView2Setup.exe"
      SetOutPath "$TEMP\zdm-webview2"
      File "${REPO_ROOT}\dist\MicrosoftEdgeWebView2Setup.exe"
      ExecWait '"$TEMP\zdm-webview2\MicrosoftEdgeWebView2Setup.exe" /silent /install' $1
      Delete "$TEMP\zdm-webview2\MicrosoftEdgeWebView2Setup.exe"
      RMDir "$TEMP\zdm-webview2"
      ${If} $1 != 0
        MessageBox MB_ICONEXCLAMATION|MB_OK "The WebView2 runtime could not be installed automatically.$\r$\n$\r$\nZDM needs it to display its window. Please install it from Microsoft and run setup again."
      ${EndIf}
    !else
      MessageBox MB_ICONINFORMATION|MB_YESNO|MB_DEFBUTTON1 \
        "The WebView2 runtime is not installed.$\r$\n$\r$\nZDM needs it to draw its window. Windows 11 includes it; Windows 10 does not.$\r$\n$\r$\nOpen Microsoft's download page now? You can also finish setup first and install it later." \
        IDNO +2
      ExecShell "open" "https://go.microsoft.com/fwlink/p/?LinkId=2124703"
    !endif
  ${Else}
    DetailPrint "WebView2 runtime already present ($0)."
  ${EndIf}
SectionEnd

Section "Desktop shortcut" SEC_SHORTCUT
  SetShellVarContext all
  CreateShortCut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\icons\icon.ico" 0
  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\${PRODUCT_EXE}" "" "$INSTDIR\icons\icon.ico" 0
  CreateShortCut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\icons\icon.ico" 0
SectionEnd

; Descriptions shown by the components page.
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_CORE}     "The download engine, the interface and the video downloader."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_BROWSERS} "Lets the browser hand downloads to ZDM and shows the download button over playing videos."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_WEBVIEW}  "Microsoft Edge WebView2, required to render the interface on Windows 10."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_TORRENT}  "Opens .torrent files in ZDM when you double-click them."
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_SHORTCUT} "Shortcuts on the desktop and in the Start menu."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

; ============================================================== post-install
Section -Post
  SetShellVarContext all
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr HKLM "${REG_UNINSTALL}" "DisplayName"     "${PRODUCT_NAME} (${PRODUCT_SHORT}) ${WIN_ARCH}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "DisplayIcon"     "$INSTDIR\icons\icon.ico"
  WriteRegStr HKLM "${REG_UNINSTALL}" "DisplayVersion"  "${PRODUCT_VERSION}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "Publisher"       "${PRODUCT_PUBLISHER}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "URLInfoAbout"    "${PRODUCT_URL}"
  WriteRegStr HKLM "${REG_UNINSTALL}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKLM "${REG_UNINSTALL}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${REG_UNINSTALL}" "QuietUninstallString" '"$INSTDIR\Uninstall.exe" /S'
  WriteRegDWORD HKLM "${REG_UNINSTALL}" "NoModify" 1
  WriteRegDWORD HKLM "${REG_UNINSTALL}" "NoRepair" 1

  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $0 "0x%08X" $0
  WriteRegDWORD HKLM "${REG_UNINSTALL}" "EstimatedSize" "$0"
SectionEnd

; ============================================================ policy helper
; Write the Chromium ExtensionSettings policy for every supported browser.
; `force_installed` is what makes the browser accept a self-signed CRX without
; asking the user whether they trust it.
Function WriteChromePolicy
  ; Chromium policy lives under HKLM\SOFTWARE\Policies\<Vendor>\<Product>.
  ; The old key ("...\Extensions\<id>\install_url") is not a policy path, so
  ; Edge ignored it and kept asking whether to trust the add-on.

  ; Allow installing from a file:// source (our signed CRX).
  WriteRegStr HKLM "SOFTWARE\Policies\Google\Chrome\ExtensionInstallSources" "1" "file:///*"
  WriteRegStr HKLM "SOFTWARE\Policies\Microsoft\Edge\ExtensionInstallSources" "1" "file:///*"
  WriteRegStr HKLM "SOFTWARE\Policies\BraveSoftware\Brave\ExtensionInstallSources" "1" "file:///*"
  WriteRegStr HKLM "SOFTWARE\Policies\Vivaldi\Vivaldi\ExtensionInstallSources" "1" "file:///*"

  ; Force-install and pin the toolbar button, per browser.
  ;
  ; `update_url` must be an XML update manifest, not the .crx itself — Chrome
  ; tries to parse the ZIP as XML and drops the entire policy. The installer
  ; writes extension\update.xml for exactly this field.
  WriteRegStr HKLM "SOFTWARE\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}" "installation_mode" "force_installed"
  WriteRegStr HKLM "SOFTWARE\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}" "update_url" "file:///$INSTDIR/extension/update.xml"
  WriteRegStr HKLM "SOFTWARE\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}" "toolbar_pin" "force_pinned"

  WriteRegStr HKLM "SOFTWARE\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}" "installation_mode" "force_installed"
  WriteRegStr HKLM "SOFTWARE\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}" "update_url" "file:///$INSTDIR/extension/update.xml"
  WriteRegStr HKLM "SOFTWARE\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}" "toolbar_pin" "force_pinned"

  WriteRegStr HKLM "SOFTWARE\Policies\BraveSoftware\Brave\ExtensionSettings\${CHROME_EXT_ID}" "installation_mode" "force_installed"
  WriteRegStr HKLM "SOFTWARE\Policies\BraveSoftware\Brave\ExtensionSettings\${CHROME_EXT_ID}" "update_url" "file:///$INSTDIR/extension/update.xml"

  WriteRegStr HKLM "SOFTWARE\Policies\Vivaldi\Vivaldi\ExtensionSettings\${CHROME_EXT_ID}" "installation_mode" "force_installed"
  WriteRegStr HKLM "SOFTWARE\Policies\Vivaldi\Vivaldi\ExtensionSettings\${CHROME_EXT_ID}" "update_url" "file:///$INSTDIR/extension/update.xml"

  ; Also write the per-user policy (HKCU wins and needs no elevation).
  WriteRegStr HKCU "Software\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}" "installation_mode" "force_installed"
  WriteRegStr HKCU "Software\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}" "update_url" "file:///$INSTDIR/extension/update.xml"
  WriteRegStr HKCU "Software\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}" "installation_mode" "force_installed"
  WriteRegStr HKCU "Software\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}" "update_url" "file:///$INSTDIR/extension/update.xml"
FunctionEnd

Function un.WriteChromePolicy
  ; Remove exactly the keys the installer wrote.
  DeleteRegKey HKLM "SOFTWARE\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}"
  DeleteRegKey HKLM "SOFTWARE\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}"
  DeleteRegKey HKLM "SOFTWARE\Policies\BraveSoftware\Brave\ExtensionSettings\${CHROME_EXT_ID}"
  DeleteRegKey HKLM "SOFTWARE\Policies\Vivaldi\Vivaldi\ExtensionSettings\${CHROME_EXT_ID}"
  DeleteRegKey HKCU "Software\Policies\Google\Chrome\ExtensionSettings\${CHROME_EXT_ID}"
  DeleteRegKey HKCU "Software\Policies\Microsoft\Edge\ExtensionSettings\${CHROME_EXT_ID}"
  DeleteRegValue HKLM "SOFTWARE\Policies\Google\Chrome\ExtensionInstallSources" "1"
  DeleteRegValue HKLM "SOFTWARE\Policies\Microsoft\Edge\ExtensionInstallSources" "1"
FunctionEnd

; ================================================================ uninstall
Section "Uninstall"
  SetShellVarContext all

  nsExec::Exec 'taskkill /F /IM ${PRODUCT_EXE} /T'
  nsExec::Exec 'taskkill /F /IM ${PRODUCT_HOST_EXE} /T'
  Sleep 400

  ; Autostart + app path
  DeleteRegValue HKCU "${REG_RUN}" "ZyzoomDownloadManager"
  DeleteRegValue HKLM "${REG_RUN}" "ZyzoomDownloadManager"
  DeleteRegKey HKLM "${REG_APP_PATH}"
  DeleteRegKey HKLM "Software\${PRODUCT_SHORT}"
  DeleteRegKey HKLM "${REG_UNINSTALL}"

  Call un.WriteChromePolicy

  ; Torrent association. The ProgID is ours alone, so removing it is always
  ; safe; the `.torrent` default value is cleared only to drop the dangling
  ; pointer to that ProgID.
  DeleteRegKey HKCU "Software\Classes\ZDM.Torrent"
  DeleteRegKey HKLM "Software\Classes\ZDM.Torrent"
  DeleteRegValue HKCU "Software\Classes\.torrent" ""
  DeleteRegValue HKLM "Software\Classes\.torrent" ""

  ; The magnet: scheme belongs to us alone, so removing it is always safe. Left
  ; behind, it points at a binary that no longer exists and every magnet click
  ; fails silently.
  DeleteRegKey HKCU "Software\Classes\magnet"
  DeleteRegKey HKLM "Software\Classes\magnet"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'

  ; Native messaging registrations
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKCU "Software\Vivaldi\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKLM "Software\Google\Chrome\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKLM "Software\Microsoft\Edge\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKLM "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\${NATIVE_HOST_NAME}"
  DeleteRegKey HKLM "Software\Mozilla\NativeMessagingHosts\${NATIVE_HOST_NAME}"

  ; External extension manifests
  Delete "$LOCALAPPDATA\Google\Chrome\User Data\External Extensions\${CHROME_EXT_ID}.json"
  Delete "$LOCALAPPDATA\Microsoft\Edge\User Data\External Extensions\${CHROME_EXT_ID}.json"
  Delete "$LOCALAPPDATA\BraveSoftware\Brave-Browser\User Data\External Extensions\${CHROME_EXT_ID}.json"
  Delete "$LOCALAPPDATA\Vivaldi\User Data\External Extensions\${CHROME_EXT_ID}.json"
  Delete "$APPDATA\Mozilla\Firefox\distribution\policies.json"

  ; Shortcuts
  Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  ; Files (user downloads and settings are deliberately left in place)
  RMDir /r "$INSTDIR\icons"
  RMDir /r "$INSTDIR\extension"
  RMDir /r "$INSTDIR\manifests"
  RMDir /r "$INSTDIR\bin"
  Delete "$INSTDIR\${PRODUCT_EXE}"
  Delete "$INSTDIR\${PRODUCT_HOST_EXE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  MessageBox MB_ICONINFORMATION|MB_OK "ZDM has been removed.$\r$\n$\r$\nYour downloads and settings in %APPDATA%\ZDM were kept."
SectionEnd

; Install/uninstall functions that must run before sections.
Function .onInit
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_ICONSTOP|MB_OK "ZDM requires Windows 10 or newer."
    Abort
  ${EndIf}

  ${If} ${FileExists} "$INSTDIR\${PRODUCT_EXE}"
    MessageBox MB_ICONQUESTION|MB_YESNO "ZDM is already installed in this folder. Upgrade it?" IDYES continue
    Abort
    continue:
  ${EndIf}

  ${If} ${RunningX64}
    ${If} "${WIN_ARCH}" == "x64"
      SetRegView 64
    ${EndIf}
  ${EndIf}
FunctionEnd

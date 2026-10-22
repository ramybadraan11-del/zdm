; Zyzoom Download Manager (ZDM) — Official Windows Installer
; Developed with pride by Ramy Badraan

!include "MUI2.nsh"
!include "FileFunc.nsh"

; --------------------------------------------------
; General Definitions
; --------------------------------------------------
!define PRODUCT_NAME "Zyzoom Download Manager"
!define PRODUCT_NAME_SHORT "ZDM"
!define PRODUCT_VERSION "1.0.0"
!define PRODUCT_PUBLISHER "Ramy Badraan"
!define PRODUCT_WEB_SITE "https://zyzoom.net"
!define PRODUCT_DIR_REGKEY "Software\Microsoft\Windows\CurrentVersion\App Paths\zdm.exe"
!define PRODUCT_UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${PRODUCT_NAME}"
!define PRODUCT_AUTORUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"

; Output Installer
Name "${PRODUCT_NAME} v${PRODUCT_VERSION}"
OutFile "..\Zyzoom_Download_Manager_Setup_v1.0.0.exe"
InstallDir "$PROGRAMFILES64\Zyzoom Download Manager"
InstallDirRegKey HKLM "${PRODUCT_DIR_REGKEY}" ""
ShowInstDetails show
ShowUnInstDetails show

; Request Administrator Privileges (UAC Prompt)
RequestExecutionLevel admin

; Modern Solid Compression (Fastest + Smallest)
SetCompressor /SOLID lzma

; Icons
!define MUI_ICON "icons\icon.ico"
!define MUI_UNICON "icons\icon.ico"

; --------------------------------------------------
; Interface Settings
; --------------------------------------------------
!define MUI_ABORTWARNING
!define MUI_HEADERIMAGE
!define MUI_HEADERIMAGE_RIGHT

; Pages - Installer
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish Page - Offer Launch
!define MUI_FINISHPAGE_RUN "$INSTDIR\zdm.exe"
!define MUI_FINISHPAGE_RUN_TEXT "تشغيل Zyzoom Download Manager الآن"
!insertmacro MUI_PAGE_FINISH

; Pages - Uninstaller
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages (Arabic by default with English fallback)
!insertmacro MUI_LANGUAGE "Arabic"
!insertmacro MUI_LANGUAGE "English"

; --------------------------------------------------
; Installer Section
; --------------------------------------------------
Section "MainSection" SEC01
  SetShellVarContext all
  
  ; Stop any previous running instances cleanly
  DetailPrint "إيقاف أي نسخ نشطة سابقة من ZDM..."
  nsExec::Exec 'taskkill /F /IM zdm.exe /T'
  nsExec::Exec 'taskkill /F /IM zdm-host.exe /T'
  Sleep 500

  ; Target Installation Directory
  SetOutPath "$INSTDIR"
  SetOverwrite on

  ; Copy Core Binaries
  DetailPrint "تثبيت محرك ZDM فائق السرعة..."
  File "..\target\x86_64-pc-windows-msvc\release\zdm.exe"
  File "..\target\x86_64-pc-windows-msvc\release\zdm-host.exe"

  ; Copy Icons
  SetOutPath "$INSTDIR\icons"
  File "icons\icon.ico"

  ; Copy Browser Extension files (Chromium unpack)
  DetailPrint "تثبيت إضافة المتصفح وملفات التكامل..."
  SetOutPath "$INSTDIR\extension"
  File /r "..\apps\extension\*.*"

  ; Copy Firefox / Zen XPI extension
  SetOutPath "$INSTDIR"
  File "..\assets\zdm@zyzoom.com.xpi"

  ; Copy and configure Native Messaging Manifests
  SetOutPath "$INSTDIR\manifests"
  File "manifests\chrome.json"
  File "manifests\edge.json"
  File "manifests\firefox.json"

  ; Update absolute paths in Native Messaging Manifests
  ; Note: In Windows Native Messaging, manifest path can point directly to full path
  DetailPrint "تسجيل إضافات المتصفحات (Chrome, Edge, Brave, Firefox)..."
  
  ; 1. Chrome Native Messaging Host
  WriteRegStr HKLM "SOFTWARE\Google\Chrome\NativeMessagingHosts\com.zyzoom.zdm" "" "$INSTDIR\manifests\chrome.json"
  WriteRegStr HKLM "SOFTWARE\Google\Chrome\Extensions\cjdipciekhdnndlcpcgjocjbdijdlhnn" "update_url" "https://clients2.google.com/service/update2/crx"

  ; 2. Edge Native Messaging Host
  WriteRegStr HKLM "SOFTWARE\Microsoft\Edge\NativeMessagingHosts\com.zyzoom.zdm" "" "$INSTDIR\manifests\edge.json"
  WriteRegStr HKLM "SOFTWARE\Microsoft\Edge\Extensions\cjdipciekhdnndlcpcgjocjbdijdlhnn" "update_url" "https://clients2.google.com/service/update2/crx"

  ; 3. Brave Browser Native Messaging Host
  WriteRegStr HKLM "SOFTWARE\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.zyzoom.zdm" "" "$INSTDIR\manifests\chrome.json"

  ; 4. Firefox Native Messaging Host
  WriteRegStr HKLM "SOFTWARE\Mozilla\NativeMessagingHosts\com.zyzoom.zdm" "" "$INSTDIR\manifests\firefox.json"

  ; --------------------------------------------------
  ; 5. Autostart with System Boot (Directly to Tray Icon)
  ; --------------------------------------------------
  DetailPrint "تفعيل التشغيل التلقائي مع بدء الويندوز في شريط المهام (Tray Icon)..."
  WriteRegStr HKLM "${PRODUCT_AUTORUN_KEY}" "ZyzoomDownloadManager" '"$INSTDIR\zdm.exe" --minimized'
  WriteRegStr HKCU "${PRODUCT_AUTORUN_KEY}" "ZyzoomDownloadManager" '"$INSTDIR\zdm.exe" --minimized'

  ; App Path Registry
  WriteRegStr HKLM "${PRODUCT_DIR_REGKEY}" "" "$INSTDIR\zdm.exe"

  ; --------------------------------------------------
  ; Shortcuts
  ; --------------------------------------------------
  DetailPrint "إنشاء الاختصارات في قائمة ابدأ وسطح المكتب..."
  CreateDirectory "$SMPROGRAMS\Zyzoom Download Manager"
  CreateShortCut "$SMPROGRAMS\Zyzoom Download Manager\Zyzoom Download Manager.lnk" "$INSTDIR\zdm.exe" "" "$INSTDIR\icons\icon.ico" 0
  CreateShortCut "$SMPROGRAMS\Zyzoom Download Manager\إلغاء التثبيت Uninstall.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0
  CreateShortCut "$DESKTOP\Zyzoom Download Manager.lnk" "$INSTDIR\zdm.exe" "" "$INSTDIR\icons\icon.ico" 0

SectionEnd

; --------------------------------------------------
; Post-Install: Write Uninstaller & Register with Windows
; --------------------------------------------------
Section -Post
  SetShellVarContext all
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayName" "${PRODUCT_NAME} (ZDM)"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayIcon" "$INSTDIR\icons\icon.ico"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "DisplayVersion" "${PRODUCT_VERSION}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "URLInfoAbout" "${PRODUCT_WEB_SITE}"
  WriteRegStr HKLM "${PRODUCT_UNINST_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
  WriteRegDWORD HKLM "${PRODUCT_UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKLM "${PRODUCT_UNINST_KEY}" "NoRepair" 1
SectionEnd

; --------------------------------------------------
; Uninstaller Section
; --------------------------------------------------
Section Uninstall
  SetShellVarContext all

  ; Close running instances
  nsExec::Exec 'taskkill /F /IM zdm.exe /T'
  nsExec::Exec 'taskkill /F /IM zdm-host.exe /T'

  ; Delete Registry Keys
  DeleteRegValue HKLM "${PRODUCT_AUTORUN_KEY}" "ZyzoomDownloadManager"
  DeleteRegValue HKCU "${PRODUCT_AUTORUN_KEY}" "ZyzoomDownloadManager"
  DeleteRegKey HKLM "${PRODUCT_DIR_REGKEY}"
  DeleteRegKey HKLM "${PRODUCT_UNINST_KEY}"

  ; Remove Native Messaging Hosts
  DeleteRegKey HKLM "SOFTWARE\Google\Chrome\NativeMessagingHosts\com.zyzoom.zdm"
  DeleteRegKey HKLM "SOFTWARE\Microsoft\Edge\NativeMessagingHosts\com.zyzoom.zdm"
  DeleteRegKey HKLM "SOFTWARE\BraveSoftware\Brave-Browser\NativeMessagingHosts\com.zyzoom.zdm"
  DeleteRegKey HKLM "SOFTWARE\Mozilla\NativeMessagingHosts\com.zyzoom.zdm"

  ; Delete Shortcuts
  Delete "$DESKTOP\Zyzoom Download Manager.lnk"
  Delete "$SMPROGRAMS\Zyzoom Download Manager\Zyzoom Download Manager.lnk"
  Delete "$SMPROGRAMS\Zyzoom Download Manager\إلغاء التثبيت Uninstall.lnk"
  RMDir "$SMPROGRAMS\Zyzoom Download Manager"

  ; Delete Installed Files
  Delete "$INSTDIR\zdm.exe"
  Delete "$INSTDIR\zdm-host.exe"
  Delete "$INSTDIR\zdm@zyzoom.com.xpi"
  Delete "$INSTDIR\icons\icon.ico"
  RMDir "$INSTDIR\icons"
  RMDir /r "$INSTDIR\extension"
  RMDir /r "$INSTDIR\manifests"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"

  SetAutoClose true
SectionEnd

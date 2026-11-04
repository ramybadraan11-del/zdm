<#
.SYNOPSIS
    Build the complete Zyzoom Download Manager (ZDM) Windows installer.

.DESCRIPTION
    Produces a single, full installer (not a portable copy):
      dist\Zyzoom_Download_Manager_Setup_<version>.exe

    Steps:
      1. verify the toolchain,
      2. build the React frontend,
      3. build zdm.exe and zdm-host.exe,
      4. package the signed CRX3 and the Firefox XPI,
      5. fetch yt-dlp and the WebView2 bootstrapper to bundle,
      6. generate the NSIS artwork,
      7. compile the installer with makensis.

.PARAMETER SkipBuild
    Reuse existing release binaries instead of rebuilding.

.PARAMETER Version
    Override the version (defaults to tauri.conf.json).

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File windows\build-installer.ps1
#>

[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

function Write-Step($message) { Write-Host "`n==> $message" -ForegroundColor Cyan }
function Write-Ok($message)   { Write-Host "    OK  $message" -ForegroundColor Green }
function Write-Warn2($message){ Write-Host "    !   $message" -ForegroundColor Yellow }
function Fail($message)       { Write-Host "`nERROR: $message" -ForegroundColor Red; exit 1 }

# ------------------------------------------------------------------ toolchain
Write-Step 'Checking the toolchain'

foreach ($tool in @('node', 'npm', 'cargo', 'rustc')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        Fail "$tool not found on PATH. Install Node.js and Rust (rustup) first."
    }
    Write-Ok "$tool $(& $tool --version | Select-Object -First 1)"
}

$Makensis = Get-Command 'makensis' -ErrorAction SilentlyContinue
if (-not $Makensis) {
    $candidates = @(
        "$env:ProgramFiles\NSIS\makensis.exe",
        "${env:ProgramFiles(x86)}\NSIS\makensis.exe"
    )
    $Makensis = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $Makensis) {
    Fail @'
makensis (NSIS 3) was not found.
Install it with one of:
  winget install NSIS.NSIS
  choco install nsis
  scoop install nsis
'@
}
Write-Ok "NSIS: $Makensis"

if (-not $Version) {
    $Version = (Get-Content 'apps\desktop\src-tauri\tauri.conf.json' -Raw | ConvertFrom-Json).version
}
Write-Ok "version $Version"

# ---------------------------------------------------------------- directories
Write-Step 'Preparing output folders'
New-Item -ItemType Directory -Force -Path 'dist' | Out-Null
New-Item -ItemType Directory -Force -Path 'windows\icons' | Out-Null
Write-Ok 'dist\, windows\icons\'

# ------------------------------------------------------------------- frontend
if (-not $SkipBuild) {
    Write-Step 'Building the frontend'
    Push-Location 'apps\desktop'
    if (-not (Test-Path 'node_modules')) {
        npm install --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { Pop-Location; Fail 'npm install failed' }
    }
    npm run build
    if ($LASTEXITCODE -ne 0) { Pop-Location; Fail 'frontend build failed' }
    Pop-Location
    Write-Ok 'apps\desktop\dist'

    Write-Step 'Building zdm.exe (Tauri, Windows target)'
    Push-Location 'apps\desktop'
    npx tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) { Pop-Location; Fail 'tauri build failed' }
    Pop-Location

    Write-Step 'Building zdm-host.exe'
    cargo build --release -p zdm-host
    if ($LASTEXITCODE -ne 0) { Fail 'zdm-host build failed' }
}

$ExePath  = 'target\release\zdm.exe'
$HostPath = 'target\release\zdm-host.exe'
if (-not (Test-Path $ExePath))  { Fail "$ExePath is missing; run without -SkipBuild" }
if (-not (Test-Path $HostPath)) { Fail "$HostPath is missing; run without -SkipBuild" }
Write-Ok "$ExePath"
Write-Ok "$HostPath"

# ------------------------------------------------------------------ extension
Write-Step 'Packaging the browser extension (CRX3 + XPI)'
$extId = 'jgoccphfikadefpdemmaohkpbomgjcak'
$havePython = Get-Command 'python' -ErrorAction SilentlyContinue
if ($havePython) {
    & python tools\build-crx.py
    if ($LASTEXITCODE -ne 0) { Write-Warn2 'CRX build failed; auto-install will be skipped' }
} else {
    Write-Warn2 'Python not found: skipping the signed CRX (unpacked install still works)'
}

# The installer expects both artefacts inside dist\.
foreach ($file in @("assets\$extId.crx", 'assets\zdm@zyzoom.com.xpi')) {
    if (Test-Path $file) {
        Copy-Item $file 'dist\' -Force
        Write-Ok (Split-Path -Leaf $file)
    } else {
        Write-Warn2 "$file not produced"
    }
}

# ------------------------------------------------------------------- binaries
Write-Step 'Fetching bundled helpers'
$YtDlp = 'dist\yt-dlp.exe'
if (-not (Test-Path $YtDlp)) {
    try {
        Invoke-WebRequest -Uri 'https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe' `
                          -OutFile $YtDlp -UseBasicParsing
        Write-Ok "yt-dlp.exe ($([math]::Round((Get-Item $YtDlp).Length / 1MB, 1)) MB)"
    } catch {
        Write-Warn2 "could not download yt-dlp: $($_.Exception.Message)"
        Write-Warn2 'The app will fetch it on first video download instead.'
    }
} else {
    Write-Ok 'yt-dlp.exe (cached)'
}

# WebView2 is bundled by default so the installer is a single self-contained file
# that works even on a fresh Windows 10 without internet. Set $env:ZDM_SLIM_INSTALLER
# = '1' to produce the smaller online installer instead.
$WebView = 'dist\MicrosoftEdgeWebView2Setup.exe'
if ($env:ZDM_SLIM_INSTALLER -ne '1') {
    if (-not (Test-Path $WebView)) {
        try {
            Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/?linkid=2124701' `
                              -OutFile $WebView -UseBasicParsing
            Write-Ok 'MicrosoftEdgeWebView2Setup.exe bundled (offline install)'
        } catch {
            Write-Warn2 'could not fetch the WebView2 bootstrapper'
        }
    } else {
        Write-Ok 'MicrosoftEdgeWebView2Setup.exe (cached)'
    }
} else {
    Write-Ok 'WebView2 not bundled (set ZDM_BUNDLE_WEBVIEW2=1 for offline installs)'
    if (Test-Path $WebView) { Remove-Item $WebView -Force }
}

# ---------------------------------------------------------------------- icons
Write-Step 'Generating installer artwork'
if (Get-Command 'python' -ErrorAction SilentlyContinue) {
    & python tools\build-nsis-assets.py
    if ($LASTEXITCODE -ne 0) { Write-Warn2 'artwork not regenerated; using the committed bitmaps' }
}
foreach ($bmp in @('windows\icons\header.bmp', 'windows\icons\wizard.bmp')) {
    if (Test-Path $bmp) { Write-Ok $bmp } else { Fail "$bmp is missing" }
}
if (-not (Test-Path 'windows\icons\icon.ico')) { Fail 'windows\icons\icon.ico is missing' }
if (-not (Test-Path 'windows\license.txt')) {
    Copy-Item 'packaging\LICENSE.txt' 'windows\license.txt' -Force
}
Write-Ok 'windows\icons\icon.ico'

# ---------------------------------------------------------------- compile NSIS
Write-Step 'Compiling the installer'
& $Makensis /V2 windows\installer.nsi
if ($LASTEXITCODE -ne 0) { Fail 'makensis failed' }

$OutFile = Get-ChildItem 'dist' -Filter 'Zyzoom_Download_Manager_Setup_*.exe' |
           Sort-Object LastWriteTime -Descending | Select-Object -First 1

if (-not $OutFile) { Fail 'installer was not produced' }

$sizeMb = [math]::Round($OutFile.Length / 1MB, 1)
$hash = (Get-FileHash $OutFile.FullName -Algorithm SHA256).Hash

Write-Host "`n$('=' * 68)" -ForegroundColor Green
Write-Host "  Installer ready" -ForegroundColor Green
Write-Host "$('=' * 68)" -ForegroundColor Green
Write-Host "  file   : $($OutFile.FullName)"
Write-Host "  size   : $sizeMb MB"
Write-Host "  sha256 : $hash"
Write-Host "  version: $Version"
Write-Host ''
Write-Host '  Includes: download engine, native host, browser integration,'
Write-Host '            video downloader, autostart, WebView2 bootstrap.'
Write-Host ''

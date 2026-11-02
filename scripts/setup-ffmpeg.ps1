# Downloads the FFmpeg shared LGPL build (BtbN) into vendor/ffmpeg and prints
# the environment variables needed to build vyu.
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/setup-ffmpeg.ps1

$ErrorActionPreference = 'Stop'

$Version = 'n9.0'
$Asset   = "ffmpeg-$Version-latest-win64-lgpl-shared-9.0"
$Url     = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/$Asset.zip"

$Root    = Split-Path -Parent $PSScriptRoot
$Vendor  = Join-Path $Root 'vendor'
$Target  = Join-Path $Vendor 'ffmpeg'
$Zip     = Join-Path $Vendor "$Asset.zip"

if (Test-Path (Join-Path $Target 'bin\avcodec-*.dll')) {
    Write-Host "FFmpeg already present in $Target"
} else {
    New-Item -ItemType Directory -Force $Vendor | Out-Null
    Write-Host "Downloading $Url"
    Invoke-WebRequest -Uri $Url -OutFile $Zip
    Expand-Archive -Path $Zip -DestinationPath $Vendor -Force
    if (Test-Path $Target) { Remove-Item -Recurse -Force $Target }
    Move-Item (Join-Path $Vendor $Asset) $Target
    Remove-Item $Zip
    Write-Host "FFmpeg extracted to $Target"
}

# libclang is required by bindgen (ffmpeg-sys-next). Install with:
#   python -m pip install --user libclang
$Clang = $env:LIBCLANG_PATH
if (-not $Clang) {
    try {
        $Clang = (python -c "import clang, os; print(os.path.join(os.path.dirname(clang.__file__), 'native'))").Trim()
    } catch { $Clang = '' }
}

Write-Host ""
Write-Host "Set these before running cargo (the build scripts read them):"
Write-Host "  `$env:FFMPEG_DIR = '$Target'"
Write-Host "  `$env:LIBCLANG_PATH = '$Clang'"
Write-Host "  `$env:PATH = '$Target\bin;' + `$env:PATH"

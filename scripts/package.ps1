# Builds vyu in release mode and packages it with the FFmpeg DLLs:
#   dist\vyu\               (ready to run)
#   dist\vyu-windows-x64.zip
#
# Usage: powershell -ExecutionPolicy Bypass -File scripts/package.ps1

$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

# FFmpeg (vendor/ffmpeg) and libclang are needed to build.
& (Join-Path $PSScriptRoot 'setup-ffmpeg.ps1') | Out-Null
$env:FFMPEG_DIR = Join-Path $Root 'vendor\ffmpeg'
if (-not $env:LIBCLANG_PATH) {
    $env:LIBCLANG_PATH = (python -c "import clang, os; print(os.path.join(os.path.dirname(clang.__file__), 'native'))").Trim()
}

cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build falhou" }

$Dist = Join-Path $Root 'dist'
$Out = Join-Path $Dist 'vyu'
if (Test-Path $Out) { Remove-Item -Recurse -Force $Out }
New-Item -ItemType Directory -Force $Out | Out-Null

Copy-Item target\release\vyu.exe $Out
# Only the FFmpeg libraries vyu links against.
foreach ($lib in 'avcodec', 'avformat', 'avutil', 'swresample', 'swscale') {
    Copy-Item "vendor\ffmpeg\bin\$lib-*.dll" $Out
}
Copy-Item LICENSE, THIRD_PARTY.md, README.md $Out

$Zip = Join-Path $Dist 'vyu-windows-x64.zip'
if (Test-Path $Zip) { Remove-Item $Zip }
Compress-Archive -Path (Join-Path $Out '*') -DestinationPath $Zip
Write-Host "Pronto: $Zip"

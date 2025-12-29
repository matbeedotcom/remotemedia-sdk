# Setup script to download FFmpeg for Windows builds
# Run this once before building: .\setup-ffmpeg.ps1

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$vendorDir = Join-Path $scriptDir "vendor"
$ffmpegDir = Join-Path $vendorDir "ffmpeg"
$includeDir = Join-Path $ffmpegDir "include"
$libDir = Join-Path $ffmpegDir "lib"

# Function to create cargo config
function New-CargoConfig {
    $cargoDir = Join-Path $scriptDir ".cargo"
    New-Item -ItemType Directory -Force -Path $cargoDir | Out-Null

    $configPath = Join-Path $cargoDir "config.toml"
    $includeEscaped = $includeDir -replace '\\', '/'
    $libEscaped = $libDir -replace '\\', '/'
    $configContent = @"
[env]
FFMPEG_INCLUDE_DIR = "$includeEscaped"
FFMPEG_LIB_DIR = "$libEscaped"
"@
    Set-Content -Path $configPath -Value $configContent
    Write-Host "  Cargo config: $configPath" -ForegroundColor Gray
}

# Check if already downloaded
if ((Test-Path (Join-Path $includeDir "libavcodec")) -and (Test-Path $libDir)) {
    Write-Host "FFmpeg already installed at $ffmpegDir" -ForegroundColor Green
    # Still create/update cargo config
    New-CargoConfig
    exit 0
}

Write-Host "Downloading FFmpeg for Windows..." -ForegroundColor Cyan

# Create directories
New-Item -ItemType Directory -Force -Path $vendorDir | Out-Null

# Use "full_build" which includes development headers and libs
$downloadUrl = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-full-shared.7z"
$archivePath = Join-Path $vendorDir "ffmpeg.7z"
$extractDir = Join-Path $vendorDir "ffmpeg-extract"

# Download
Write-Host "Downloading from $downloadUrl..."
try {
    Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -UseBasicParsing
} catch {
    Write-Host "Invoke-WebRequest failed, trying curl..." -ForegroundColor Yellow
    curl -L -o $archivePath $downloadUrl
}

if (-not (Test-Path $archivePath)) {
    Write-Error "Failed to download FFmpeg"
    exit 1
}

# Extract (7z file)
Write-Host "Extracting..."
if (Test-Path $extractDir) {
    Remove-Item -Recurse -Force $extractDir
}

# Try 7z, then tar (Windows 10+ has tar with 7z support via bsdtar)
$extracted = $false
if (Get-Command "7z" -ErrorAction SilentlyContinue) {
    & 7z x $archivePath -o"$extractDir" -y
    $extracted = $true
} elseif (Get-Command "7za" -ErrorAction SilentlyContinue) {
    & 7za x $archivePath -o"$extractDir" -y
    $extracted = $true
} else {
    # Fallback: download zip version instead
    Write-Host "7z not found, downloading zip version instead..." -ForegroundColor Yellow
    Remove-Item -Force $archivePath -ErrorAction SilentlyContinue
    $downloadUrl = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip"
    $archivePath = Join-Path $vendorDir "ffmpeg.zip"
    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -UseBasicParsing
    } catch {
        curl -L -o $archivePath $downloadUrl
    }
    Expand-Archive -Path $archivePath -DestinationPath $extractDir -Force
    $extracted = $true
}

if (-not $extracted) {
    Write-Error "Failed to extract FFmpeg. Please install 7-Zip or use a zip download."
    exit 1
}

# Find extracted folder (ffmpeg-VERSION-full_build-shared)
$extractedFolder = Get-ChildItem -Path $extractDir -Directory | Where-Object { $_.Name -like "ffmpeg-*" } | Select-Object -First 1

if (-not $extractedFolder) {
    Write-Error "Could not find extracted FFmpeg folder"
    exit 1
}

# Create target directory
if (Test-Path $ffmpegDir) {
    Remove-Item -Recurse -Force $ffmpegDir
}
New-Item -ItemType Directory -Force -Path $ffmpegDir | Out-Null

# Copy include and lib directories
Write-Host "Copying include and lib directories..."
Copy-Item -Path (Join-Path $extractedFolder.FullName "include") -Destination $ffmpegDir -Recurse
Copy-Item -Path (Join-Path $extractedFolder.FullName "lib") -Destination $ffmpegDir -Recurse

# Also copy bin for runtime DLLs
if (Test-Path (Join-Path $extractedFolder.FullName "bin")) {
    Copy-Item -Path (Join-Path $extractedFolder.FullName "bin") -Destination $ffmpegDir -Recurse
    Write-Host "Note: FFmpeg DLLs copied to $ffmpegDir\bin - add to PATH or copy to output directory for runtime" -ForegroundColor Yellow
}

# Cleanup
Write-Host "Cleaning up..."
Remove-Item -Force $archivePath -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force $extractDir -ErrorAction SilentlyContinue

# Create .cargo/config.toml with FFmpeg paths so cargo picks them up automatically
New-CargoConfig

Write-Host ""
Write-Host "FFmpeg installed successfully!" -ForegroundColor Green
Write-Host "  Include: $includeDir"
Write-Host "  Lib: $libDir"
Write-Host ""
Write-Host "You can now build with: cargo build" -ForegroundColor Cyan

# Setup script to build FFmpeg from source with STATIC linking for Windows (MSVC)
# Run this once before building: .\setup-ffmpeg.ps1
#
# This builds FFmpeg as static libraries using MSVC toolchain so binaries are self-contained
# (no runtime DLL dependencies) and compatible with Rust's MSVC target.
#
# Prerequisites:
#   - Visual Studio 2019/2022 with C++ build tools
#   - MSYS2 (for configure script and make)
#   - Run from Developer PowerShell or after vcvars64.bat

$ErrorActionPreference = "Stop"
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$vendorDir = Join-Path $scriptDir "vendor"
$ffmpegDir = Join-Path $vendorDir "ffmpeg"
$includeDir = Join-Path $ffmpegDir "include"
$libDir = Join-Path $ffmpegDir "lib"

# FFmpeg version to build
$FFMPEG_VERSION = "7.1"
$FFMPEG_URL = "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"

# Function to update cargo config for static FFmpeg linking (preserves existing content)
function Update-CargoConfig {
    $cargoDir = Join-Path $scriptDir ".cargo"
    New-Item -ItemType Directory -Force -Path $cargoDir | Out-Null

    $configPath = Join-Path $cargoDir "config.toml"
    $includeEscaped = $includeDir -replace '\\', '/'
    $libEscaped = $libDir -replace '\\', '/'

    # Read existing config or start fresh
    $existingContent = ""
    if (Test-Path $configPath) {
        $existingContent = Get-Content -Path $configPath -Raw
    }

    # Remove any existing FFmpeg-related lines
    $lines = $existingContent -split "`n" | Where-Object {
        $_ -notmatch "^FFMPEG_INCLUDE_DIR\s*=" -and
        $_ -notmatch "^FFMPEG_LIB_DIR\s*=" -and
        $_ -notmatch "^FFMPEG_LIBS_MODE\s*=" -and
        $_ -notmatch "^#.*FFmpeg.*static" -and
        $_ -notmatch "^#.*static linking for FFmpeg"
    }

    # Find or create [env] section
    $hasEnvSection = $lines | Where-Object { $_ -match "^\[env\]" }

    if (-not $hasEnvSection) {
        # Add [env] section at the beginning if it doesn't exist
        $ffmpegConfig = @"
[env]
FFMPEG_INCLUDE_DIR = "$includeEscaped"
FFMPEG_LIB_DIR = "$libEscaped"
# Use static linking for FFmpeg (single binary, no DLL dependencies)
FFMPEG_LIBS_MODE = "static"

"@
        $newContent = $ffmpegConfig + ($lines -join "`n")
    } else {
        # Insert FFmpeg vars after [env] section
        $newLines = @()
        $insertedFfmpeg = $false
        foreach ($line in $lines) {
            $newLines += $line
            if ($line -match "^\[env\]" -and -not $insertedFfmpeg) {
                $newLines += "FFMPEG_INCLUDE_DIR = `"$includeEscaped`""
                $newLines += "FFMPEG_LIB_DIR = `"$libEscaped`""
                $newLines += "# Use static linking for FFmpeg (single binary, no DLL dependencies)"
                $newLines += "FFMPEG_LIBS_MODE = `"static`""
                $insertedFfmpeg = $true
            }
        }
        $newContent = $newLines -join "`n"
    }

    # Clean up multiple blank lines
    $newContent = $newContent -replace "`n{3,}", "`n`n"
    $newContent = $newContent.Trim() + "`n"

    Set-Content -Path $configPath -Value $newContent -NoNewline
    Write-Host "  Cargo config updated: $configPath" -ForegroundColor Gray
}

# Check if already built (look for .lib files)
if (Test-Path (Join-Path $libDir "avcodec.lib")) {
    Write-Host "FFmpeg static libraries already built at $ffmpegDir" -ForegroundColor Green
    Update-CargoConfig
    exit 0
}

# Check if .a files exist (needs renaming)
if (Test-Path (Join-Path $libDir "libavcodec.a")) {
    Write-Host "FFmpeg libraries need renaming from .a to .lib..." -ForegroundColor Yellow
    $libsToRename = @("libavcodec", "libavdevice", "libavfilter", "libavformat", "libavutil", "libpostproc", "libswresample", "libswscale")
    foreach ($lib in $libsToRename) {
        $aFile = Join-Path $libDir "$lib.a"
        $libFile = Join-Path $libDir "$($lib -replace '^lib', '').lib"
        if (Test-Path $aFile) {
            Move-Item -Force $aFile $libFile
            Write-Host "  $lib.a -> $($lib -replace '^lib', '').lib" -ForegroundColor Gray
        }
    }
    Update-CargoConfig
    exit 0
}


Write-Host "Building FFmpeg $FFMPEG_VERSION from source (STATIC, MSVC)..." -ForegroundColor Cyan
Write-Host ""

# Check for MSVC environment
if (-not $env:VSINSTALLDIR) {
    Write-Host "Visual Studio environment not detected. Initializing..." -ForegroundColor Yellow

    $vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vsWhere) {
        $vsPath = & $vsWhere -latest -property installationPath
        $vcvars = Join-Path $vsPath "VC\Auxiliary\Build\vcvars64.bat"
        if (Test-Path $vcvars) {
            Write-Host "Loading MSVC environment from: $vcvars" -ForegroundColor Gray
            cmd /c "`"$vcvars`" && set" | ForEach-Object {
                if ($_ -match '^([^=]+)=(.*)$') {
                    [System.Environment]::SetEnvironmentVariable($matches[1], $matches[2])
                }
            }
        }
    }

    if (-not $env:VSINSTALLDIR) {
        Write-Error @"
MSVC environment not found. Please either:
  1. Run this script from Developer PowerShell for VS 2022
  2. Or run: & 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat'
"@
        exit 1
    }
}

Write-Host "Using MSVC: $env:VCToolsVersion" -ForegroundColor Gray

# Check prerequisites - MSYS2
$msys2Path = $null
$msys2Locations = @(
    "C:\msys64",
    "C:\tools\msys64",
    "$env:USERPROFILE\scoop\apps\msys2\current"
)

foreach ($loc in $msys2Locations) {
    if (Test-Path (Join-Path $loc "usr\bin\bash.exe")) {
        $msys2Path = $loc
        break
    }
}

if (-not $msys2Path) {
    Write-Host "MSYS2 not found. Installing via winget..." -ForegroundColor Yellow
    winget install --id MSYS2.MSYS2 --accept-package-agreements --accept-source-agreements
    $msys2Path = "C:\msys64"

    if (-not (Test-Path $msys2Path)) {
        Write-Error @"
MSYS2 is required to build FFmpeg. Please install it manually:
  1. Download from https://www.msys2.org/
  2. Install to C:\msys64
  3. Run this script again
"@
        exit 1
    }
}

Write-Host "Using MSYS2 at: $msys2Path" -ForegroundColor Gray

# Create directories
New-Item -ItemType Directory -Force -Path $vendorDir | Out-Null
New-Item -ItemType Directory -Force -Path $ffmpegDir | Out-Null

# Download FFmpeg source
$archivePath = Join-Path $vendorDir "ffmpeg-$FFMPEG_VERSION.tar.xz"
if (-not (Test-Path $archivePath)) {
    Write-Host "Downloading FFmpeg $FFMPEG_VERSION source..."
    $ProgressPreference = 'SilentlyContinue'
    try {
        Invoke-WebRequest -Uri $FFMPEG_URL -OutFile $archivePath -UseBasicParsing -TimeoutSec 300
    } catch {
        Write-Host "Trying curl.exe..." -ForegroundColor Yellow
        & curl.exe -L -o $archivePath $FFMPEG_URL
    }
}

# Convert Windows paths to MSYS2 paths for the build script
$vendorDirMsys = $vendorDir -replace '\\', '/' -replace '^C:', '/c'
$ffmpegDirMsys = $ffmpegDir -replace '\\', '/' -replace '^C:', '/c'

# Verify MSVC is available
$clPath = (Get-Command cl.exe -ErrorAction SilentlyContinue).Source
if (-not $clPath) {
    Write-Error "cl.exe not found in PATH. Ensure MSVC environment is loaded."
    exit 1
}
Write-Host "Found cl.exe at: $clPath" -ForegroundColor Gray

# Create build script for MSYS2 with MSVC toolchain
$buildScript = @'
#!/bin/bash
set -e

echo "=== FFmpeg Static Build for Windows (MSVC) ==="

# Install MSYS2 build dependencies (not MinGW - we use MSVC)
pacman -S --needed --noconfirm \
    make \
    diffutils \
    pkg-config \
    tar \
    xz \
    nasm \
    yasm

# Verify cl.exe is accessible (should be inherited from PowerShell via MSYS2_PATH_TYPE=inherit)
if ! command -v cl.exe &> /dev/null; then
    echo "ERROR: cl.exe not found in PATH"
    echo "Make sure to run this from Developer PowerShell or after vcvars64.bat"
    exit 1
fi

echo "Using MSVC compiler: $(which cl.exe)"

# Extract source
cd "VENDOR_DIR_PLACEHOLDER"
if [ ! -d "ffmpeg-FFMPEG_VERSION_PLACEHOLDER" ]; then
    echo "Extracting FFmpeg source..."
    tar -xf ffmpeg-FFMPEG_VERSION_PLACEHOLDER.tar.xz
fi

cd ffmpeg-FFMPEG_VERSION_PLACEHOLDER

# Clean previous build if exists
if [ -f "config.h" ]; then
    echo "Cleaning previous build..."
    make clean || true
fi

# Configure for MSVC toolchain
# --toolchain=msvc tells FFmpeg to use cl.exe, link.exe, lib.exe
./configure \
    --prefix="FFMPEG_DIR_PLACEHOLDER" \
    --toolchain=msvc \
    --enable-static \
    --disable-shared \
    --disable-programs \
    --disable-doc \
    --disable-debug \
    --enable-gpl \
    --enable-version3 \
    --enable-nonfree \
    --disable-network \
    --disable-protocols \
    --enable-protocol=file \
    --disable-devices \
    --disable-filters \
    --enable-filter=aresample \
    --enable-filter=anull \
    --enable-filter=null \
    --arch=x86_64

# Note: This enables all built-in codecs including:
# Video: H.264, H.265/HEVC, VP8, VP9, AV1, MPEG-1/2/4, ProRes, DNxHD, etc.
# Audio: AAC, MP3, Opus, Vorbis, FLAC, AC3, DTS, PCM variants, etc.
# Containers: MP4, MKV, WebM, AVI, MOV, FLV, etc.

echo "Building FFmpeg with MSVC (this may take 10-20 minutes)..."
make -j$(nproc)

echo "Installing to prefix..."
make install

echo "=== FFmpeg build complete ==="
'@

# Replace placeholders with actual values
$buildScript = $buildScript -replace 'VENDOR_DIR_PLACEHOLDER', $vendorDirMsys
$buildScript = $buildScript -replace 'FFMPEG_DIR_PLACEHOLDER', $ffmpegDirMsys
$buildScript = $buildScript -replace 'FFMPEG_VERSION_PLACEHOLDER', $FFMPEG_VERSION

$buildScriptPath = Join-Path $vendorDir "build-ffmpeg.sh"
$buildScript = $buildScript -replace '\r\n', "`n"
[System.IO.File]::WriteAllText($buildScriptPath, $buildScript, [System.Text.UTF8Encoding]::new($false))

# Run build using MSYS2 bash with MSVC environment inherited
Write-Host "Starting FFmpeg build with MSVC (this may take 10-20 minutes)..." -ForegroundColor Yellow

$msysBash = Join-Path $msys2Path "usr\bin\bash.exe"

# Convert Windows path to MSYS2 path
$buildScriptMsys = $buildScriptPath -replace '\\', '/' -replace '^C:', '/c'

# Set up MSYS2 environment to inherit Windows PATH (which has MSVC)
$env:MSYSTEM = "MSYS"
$env:CHERE_INVOKING = "1"
$env:MSYS2_PATH_TYPE = "inherit"

# Run the build script
Write-Host "Running: bash $buildScriptMsys" -ForegroundColor Gray
& $msysBash --login -c "cd '$vendorDirMsys' && bash '$buildScriptMsys'"

if ($LASTEXITCODE -ne 0) {
    Write-Error "FFmpeg build failed with exit code $LASTEXITCODE"
    exit 1
}

# FFmpeg with --toolchain=msvc produces .a files (Unix naming) but they are MSVC-compatible
# Rename them to .lib for Windows toolchain compatibility
Write-Host "Renaming libraries from .a to .lib format..." -ForegroundColor Gray

$libsToRename = @("libavcodec", "libavdevice", "libavfilter", "libavformat", "libavutil", "libpostproc", "libswresample", "libswscale")
foreach ($lib in $libsToRename) {
    $aFile = Join-Path $libDir "$lib.a"
    $libFile = Join-Path $libDir "$($lib -replace '^lib', '').lib"
    if (Test-Path $aFile) {
        Move-Item -Force $aFile $libFile
        Write-Host "  $lib.a -> $($lib -replace '^lib', '').lib" -ForegroundColor Gray
    }
}

# Verify build
if (-not (Test-Path (Join-Path $libDir "avcodec.lib"))) {
    Write-Error "Build completed but avcodec.lib not found. Check build output."
    exit 1
}

# Update cargo config
Update-CargoConfig

Write-Host ""
Write-Host "FFmpeg $FFMPEG_VERSION built successfully (STATIC, MSVC)!" -ForegroundColor Green
Write-Host "  Include: $includeDir"
Write-Host "  Lib: $libDir"
Write-Host ""
Write-Host "Static libraries created - no runtime DLLs needed!" -ForegroundColor Cyan

# Build script for Voice Assistant with CUDA support
# This script sets up the Visual Studio and CUDA environment, then builds

$ErrorActionPreference = "Stop"

Write-Host "Setting up Visual Studio environment..." -ForegroundColor Cyan

# Visual Studio paths
$vsBase = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools"
$msvcVersion = "14.44.35207"
$msvcBin = "$vsBase\VC\Tools\MSVC\$msvcVersion\bin\HostX64\x64"
$windowsKitBin = "C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64"

# Set Visual Studio environment variables directly (avoid vcvars64.bat PATH explosion)
$env:VSINSTALLDIR = "$vsBase\"
$env:VCINSTALLDIR = "$vsBase\VC\"
$env:VCToolsInstallDir = "$vsBase\VC\Tools\MSVC\$msvcVersion\"
$env:VCToolsVersion = $msvcVersion
$env:VSCMD_ARG_TGT_ARCH = "x64"

# Include paths for MSVC
$env:INCLUDE = "$vsBase\VC\Tools\MSVC\$msvcVersion\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um"
$env:LIB = "$vsBase\VC\Tools\MSVC\$msvcVersion\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64"

# Set minimal PATH for MSVC tools
$env:PATH = "$msvcBin;$windowsKitBin;$env:SystemRoot\System32;$env:USERPROFILE\.cargo\bin;$env:USERPROFILE\.rustup\toolchains\stable-x86_64-pc-windows-msvc\bin"

Write-Host "Setting up CUDA 12.6 environment..." -ForegroundColor Cyan

# Set CUDA environment
$cudaPath = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6"
$env:CUDA_PATH = $cudaPath
$env:CUDA_ROOT = $cudaPath
$env:CUDA_TOOLKIT_ROOT_DIR = $cudaPath
$env:PATH = "$cudaPath\bin;$env:PATH"

# Tell nvcc to use the MSVC compiler directly (avoids vcvars64.bat re-execution issues)
$msvcBin = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64"
$env:NVCC_CCBIN = $msvcBin

# Set C runtime flags
$env:CFLAGS = "/MD"
$env:CXXFLAGS = "/MD"

# Disable vcpkg auto-detection (prevents unwanted dependencies like zlib, openssl)
Remove-Item Env:VCPKG_ROOT -ErrorAction SilentlyContinue
$env:VCPKGRS_DYNAMIC = "0"
$env:VCPKGRS_NO_FFMPEG = "1"

# Set FFmpeg paths for ac-ffmpeg crate (static linking)
$ffmpegDir = "$PSScriptRoot\..\..\..\vendor\ffmpeg"
$env:FFMPEG_INCLUDE_DIR = "$ffmpegDir\include"
$env:FFMPEG_LIB_DIR = "$ffmpegDir\lib"
$env:FFMPEG_STATIC = "1"

Write-Host "CUDA_PATH = $env:CUDA_PATH" -ForegroundColor Gray
Write-Host "FFMPEG_LIB_DIR = $env:FFMPEG_LIB_DIR" -ForegroundColor Gray

# Verify CUDA
Write-Host "`nCUDA version:" -ForegroundColor Cyan
nvcc --version

# Build
Write-Host "`nBuilding with CUDA support..." -ForegroundColor Cyan
Push-Location "$PSScriptRoot\src-tauri"
try {
    cargo build --release
    if ($LASTEXITCODE -eq 0) {
        Write-Host "`nBuild successful!" -ForegroundColor Green
        Write-Host "Binary at: $PSScriptRoot\src-tauri\target\release\voice-assistant.exe"
    } else {
        Write-Host "`nBuild failed!" -ForegroundColor Red
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

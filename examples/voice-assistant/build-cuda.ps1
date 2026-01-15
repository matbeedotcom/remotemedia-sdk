# Build script for Voice Assistant with CUDA support
# This script sets up the Visual Studio and CUDA environment, then builds

$ErrorActionPreference = "Stop"

Write-Host "Setting up Visual Studio environment..." -ForegroundColor Cyan

# Call vcvarsall.bat and capture environment changes
$vsPath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vsPath)) {
    Write-Host "Visual Studio Build Tools not found at: $vsPath" -ForegroundColor Red
    exit 1
}

# Run vcvars64.bat and capture the resulting environment
$envOutput = cmd /c "`"$vsPath`" && set"
foreach ($line in $envOutput) {
    if ($line -match "^([^=]+)=(.*)$") {
        $name = $matches[1]
        $value = $matches[2]
        [Environment]::SetEnvironmentVariable($name, $value, "Process")
    }
}

Write-Host "Setting up CUDA 12.6 environment..." -ForegroundColor Cyan

# Set CUDA environment
$cudaPath = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6"
$env:CUDA_PATH = $cudaPath
$env:CUDA_ROOT = $cudaPath
$env:CUDA_TOOLKIT_ROOT_DIR = $cudaPath
$env:PATH = "$cudaPath\bin;$env:PATH"

# Set C runtime flags
$env:CFLAGS = "/MD"
$env:CXXFLAGS = "/MD"

Write-Host "CUDA_PATH = $env:CUDA_PATH" -ForegroundColor Gray

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

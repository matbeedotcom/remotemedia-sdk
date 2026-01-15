# Development build script for Voice Assistant (Windows)
# Run from PowerShell: .\dev.ps1
# 
# This script sets up FFmpeg and runs `cargo build` for the Tauri backend.
# For full dev mode with hot-reload, use: npm run tauri dev

$ErrorActionPreference = "Stop"

# Change to script directory
Push-Location $PSScriptRoot

try {
    Write-Host "Setting up development environment..." -ForegroundColor Cyan
    
    # Check for Perl (required for OpenSSL vendored build)
    $perlPath = Get-Command perl -ErrorAction SilentlyContinue
    if (-not $perlPath) {
        # Check Strawberry Perl default location
        $strawberryPerl = "C:\Strawberry\perl\bin"
        if (Test-Path "$strawberryPerl\perl.exe") {
            Write-Host "Adding Strawberry Perl to PATH..." -ForegroundColor Yellow
            $env:PATH = "$strawberryPerl;C:\Strawberry\c\bin;$env:PATH"
            $perlPath = Get-Command perl -ErrorAction SilentlyContinue
        }
    }
    if (-not $perlPath) {
        Write-Host "`nPerl is required to build OpenSSL from source." -ForegroundColor Red
        Write-Host "Install Strawberry Perl from: https://strawberryperl.com/" -ForegroundColor Yellow
        Write-Host "Or run: winget install StrawberryPerl.StrawberryPerl" -ForegroundColor Yellow
        Write-Host "`nAfter installing, restart your terminal and run this script again.`n" -ForegroundColor Gray
        exit 1
    }
    Write-Host "Perl found: $($perlPath.Source)" -ForegroundColor Gray
    
    # Setup FFmpeg for ac-ffmpeg crate
    $ffmpegDir = Join-Path $PSScriptRoot "src-tauri\.ffmpeg"
    $ffmpegInclude = Join-Path $ffmpegDir "include"
    $ffmpegLib = Join-Path $ffmpegDir "lib"
    $ffmpegBin = Join-Path $ffmpegDir "bin"
    
    # Check if FFmpeg is already downloaded
    if (-not (Test-Path $ffmpegInclude) -or -not (Test-Path $ffmpegLib)) {
        Write-Host "Downloading FFmpeg..." -ForegroundColor Yellow
        
        # Create directory
        New-Item -ItemType Directory -Force -Path $ffmpegDir | Out-Null
        
        # Download FFmpeg full shared build from gyan.dev (includes all codecs)
        $downloadUrl = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-full-shared.7z"
        $archivePath = Join-Path $ffmpegDir "ffmpeg.7z"
        $extractDir = Join-Path $ffmpegDir "extracted"
        
        Write-Host "  Downloading from $downloadUrl..." -ForegroundColor Gray
        Invoke-WebRequest -Uri $downloadUrl -OutFile $archivePath -UseBasicParsing
        
        Write-Host "  Extracting..." -ForegroundColor Gray
        # Use 7z to extract (comes with Windows or install via: winget install 7zip.7zip)
        $7zPath = Get-Command 7z -ErrorAction SilentlyContinue
        if (-not $7zPath) {
            # Check common installation paths
            $7zLocations = @(
                "C:\Program Files\7-Zip\7z.exe",
                "C:\Program Files (x86)\7-Zip\7z.exe"
            )
            foreach ($loc in $7zLocations) {
                if (Test-Path $loc) {
                    $7zPath = $loc
                    break
                }
            }
        } else {
            $7zPath = $7zPath.Source
        }
        if (-not $7zPath) {
            Write-Host "`n7-Zip is required to extract FFmpeg." -ForegroundColor Red
            Write-Host "Install via: winget install 7zip.7zip" -ForegroundColor Yellow
            exit 1
        }
        & $7zPath x $archivePath -o"$extractDir" -y | Out-Null
        
        # Find the extracted directory (ffmpeg-VERSION-essentials_build)
        $ffmpegBuildDir = Get-ChildItem -Path $extractDir -Directory | Where-Object { $_.Name -match "^ffmpeg-" } | Select-Object -First 1
        
        if (-not $ffmpegBuildDir) {
            throw "Failed to find extracted FFmpeg directory"
        }
        
        Write-Host "  Found: $($ffmpegBuildDir.Name)" -ForegroundColor Gray
        
        # Copy include, lib, and bin directories
        Copy-Item -Path (Join-Path $ffmpegBuildDir.FullName "include") -Destination $ffmpegDir -Recurse -Force
        Copy-Item -Path (Join-Path $ffmpegBuildDir.FullName "lib") -Destination $ffmpegDir -Recurse -Force
        Copy-Item -Path (Join-Path $ffmpegBuildDir.FullName "bin") -Destination $ffmpegDir -Recurse -Force
        
        # Clean up
        Remove-Item -Path $archivePath -Force -ErrorAction SilentlyContinue
        Remove-Item -Path $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        
        Write-Host "  FFmpeg downloaded successfully!" -ForegroundColor Green
    } else {
        Write-Host "Using cached FFmpeg from $ffmpegDir" -ForegroundColor Gray
    }
    
    # Set environment variables for ac-ffmpeg
    $env:FFMPEG_INCLUDE_DIR = $ffmpegInclude
    $env:FFMPEG_LIB_DIR = $ffmpegLib
    Write-Host "FFMPEG_INCLUDE_DIR = $ffmpegInclude" -ForegroundColor Gray
    Write-Host "FFMPEG_LIB_DIR = $ffmpegLib" -ForegroundColor Gray
    
    # Add FFmpeg bin to PATH for runtime DLLs
    $env:PATH = "$ffmpegBin;$env:PATH"
    
    # Set C/C++ compiler flags to use dynamic CRT (/MD)
    # This ensures esaxx-rs (used by rwhisper) is compiled with the same CRT
    # as ONNX Runtime (ort), avoiding LNK2038 mismatch errors
    $env:CFLAGS = "/MD"
    $env:CXXFLAGS = "/MD"
    Write-Host "CFLAGS = /MD (dynamic CRT)" -ForegroundColor Gray
    Write-Host "CXXFLAGS = /MD (dynamic CRT)" -ForegroundColor Gray
    
    # Change to src-tauri directory for cargo build
    Push-Location "src-tauri"
    
    try {
        Write-Host "`nBuilding Rust backend..." -ForegroundColor Cyan
        cargo build @args
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host "`nBuild successful!" -ForegroundColor Green
        } else {
            Write-Host "`nBuild failed with exit code $LASTEXITCODE" -ForegroundColor Red
            exit $LASTEXITCODE
        }
    }
    finally {
        Pop-Location
    }
}
finally {
    Pop-Location
}

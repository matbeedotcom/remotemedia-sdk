# Setup script for RemoteMedia SDK examples (Windows)
# Run from PowerShell: .\setup-windows.ps1
#
# This script downloads FFmpeg and sets up environment variables
# required for building examples that depend on remotemedia-core with video support.

$ErrorActionPreference = "Stop"

Push-Location $PSScriptRoot

try {
    Write-Host "Setting up build environment for RemoteMedia examples..." -ForegroundColor Cyan
    
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
        Write-Host "Install Strawberry Perl:" -ForegroundColor Yellow
        Write-Host "  winget install StrawberryPerl.StrawberryPerl" -ForegroundColor White
        Write-Host "`nAfter installing, restart your terminal and run this script again.`n" -ForegroundColor Gray
        exit 1
    }
    Write-Host "Perl found: $($perlPath.Source)" -ForegroundColor Gray
    
    # Setup FFmpeg
    $ffmpegDir = Join-Path $PSScriptRoot ".ffmpeg"
    $ffmpegInclude = Join-Path $ffmpegDir "include"
    $ffmpegLib = Join-Path $ffmpegDir "lib"
    $ffmpegBin = Join-Path $ffmpegDir "bin"
    
    if (-not (Test-Path $ffmpegInclude) -or -not (Test-Path $ffmpegLib)) {
        Write-Host "Downloading FFmpeg..." -ForegroundColor Yellow
        
        New-Item -ItemType Directory -Force -Path $ffmpegDir | Out-Null
        
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
        
        $ffmpegBuildDir = Get-ChildItem -Path $extractDir -Directory | Where-Object { $_.Name -match "^ffmpeg-" } | Select-Object -First 1
        
        if (-not $ffmpegBuildDir) {
            throw "Failed to find extracted FFmpeg directory"
        }
        
        Write-Host "  Found: $($ffmpegBuildDir.Name)" -ForegroundColor Gray
        
        Copy-Item -Path (Join-Path $ffmpegBuildDir.FullName "include") -Destination $ffmpegDir -Recurse -Force
        Copy-Item -Path (Join-Path $ffmpegBuildDir.FullName "lib") -Destination $ffmpegDir -Recurse -Force
        Copy-Item -Path (Join-Path $ffmpegBuildDir.FullName "bin") -Destination $ffmpegDir -Recurse -Force
        
        Remove-Item -Path $archivePath -Force -ErrorAction SilentlyContinue
        Remove-Item -Path $extractDir -Recurse -Force -ErrorAction SilentlyContinue
        
        Write-Host "  FFmpeg downloaded successfully!" -ForegroundColor Green
    } else {
        Write-Host "Using cached FFmpeg from $ffmpegDir" -ForegroundColor Gray
    }
    
    # Set environment variables
    $env:FFMPEG_INCLUDE_DIR = $ffmpegInclude
    $env:FFMPEG_LIB_DIR = $ffmpegLib
    $env:PATH = "$ffmpegBin;$env:PATH"
    
    Write-Host "`nEnvironment configured:" -ForegroundColor Green
    Write-Host "  FFMPEG_INCLUDE_DIR = $ffmpegInclude" -ForegroundColor Gray
    Write-Host "  FFMPEG_LIB_DIR = $ffmpegLib" -ForegroundColor Gray
    
    # Now run cargo build with any provided arguments
    if ($args.Count -gt 0) {
        Write-Host "`nRunning: cargo $args" -ForegroundColor Cyan
        cargo @args
        exit $LASTEXITCODE
    } else {
        Write-Host "`nSetup complete! You can now run cargo commands in this terminal." -ForegroundColor Green
        Write-Host "Example: cargo build --release" -ForegroundColor Yellow
        Write-Host "`nOr run this script with cargo arguments:" -ForegroundColor Gray
        Write-Host "  .\setup-windows.ps1 build --release" -ForegroundColor White
    }
}
finally {
    Pop-Location
}

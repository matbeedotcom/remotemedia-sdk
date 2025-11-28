# Build script for Voice Assistant (Windows)
# Run from PowerShell: .\build.ps1

$ErrorActionPreference = "Stop"

# Change to script directory
Push-Location $PSScriptRoot

try {
    Write-Host "Building Voice Assistant..." -ForegroundColor Cyan

    # Run tauri build
    npm run tauri build @args

    if ($LASTEXITCODE -eq 0) {
        Write-Host "`nBuild successful!" -ForegroundColor Green
        Write-Host "Output: src-tauri\target\release\bundle\" -ForegroundColor Yellow
    } else {
        Write-Host "`nBuild failed with exit code $LASTEXITCODE" -ForegroundColor Red
        exit $LASTEXITCODE
    }
}
finally {
    Pop-Location
}

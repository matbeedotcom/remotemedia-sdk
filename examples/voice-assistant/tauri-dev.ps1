# Tauri development wrapper with VS/CUDA environment setup (Windows)
# Usage: .\tauri-dev.ps1 [tauri args...]
# Or via npm: npm run tauri:dev

param(
    [Parameter(ValueFromRemainingArguments=$true)]
    [string[]]$TauriArgs
)

$ErrorActionPreference = "Stop"

Write-Host "Setting up build environment for Tauri..." -ForegroundColor Cyan

# Default to 'dev' if no args provided
if ($TauriArgs.Count -eq 0) {
    $TauriArgs = @("dev")
}

$viteJob = $null

# For 'dev' command, start vite first (in current shell with normal PATH)
if ($TauriArgs[0] -eq "dev") {
    Write-Host "Starting Vite dev server..." -ForegroundColor Gray
    $viteJob = Start-Job -ScriptBlock {
        Set-Location $using:PWD
        & npm run dev
    }
    Start-Sleep -Seconds 2
    Write-Host "  Vite started (Job ID: $($viteJob.Id))" -ForegroundColor Green
}

# ============================================================================
# Build vcvars environment for cargo/tauri
# ============================================================================
$vcvarsEnv = @{}

$vsPath = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if (-not (Test-Path $vsPath)) {
    $vsEditions = @("Community", "Professional", "Enterprise")
    foreach ($edition in $vsEditions) {
        $altPath = "C:\Program Files\Microsoft Visual Studio\2022\$edition\VC\Auxiliary\Build\vcvars64.bat"
        if (Test-Path $altPath) {
            $vsPath = $altPath
            break
        }
    }
}

if (Test-Path $vsPath) {
    Write-Host "Loading Visual Studio environment..." -ForegroundColor Gray
    
    # Capture vcvars environment
    $tempBat = [System.IO.Path]::GetTempFileName() + ".bat"
    @"
@echo off
call "$vsPath" >nul 2>&1
set
"@ | Set-Content $tempBat
    
    $envOutput = & cmd /c $tempBat 2>$null
    Remove-Item $tempBat -ErrorAction SilentlyContinue
    
    foreach ($line in $envOutput) {
        if ($line -match "^([^=]+)=(.*)$") {
            $vcvarsEnv[$matches[1]] = $matches[2]
        }
    }
    Write-Host "  Visual Studio environment loaded" -ForegroundColor Green
} else {
    Write-Host "Warning: Visual Studio not found. CUDA builds may fail." -ForegroundColor Yellow
}

# Add CUDA 12.x
$cudaVersions = @("v12.6", "v12.5", "v12.4", "v12.3", "v12.2", "v12.1", "v12.0")
$cudaBase = "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA"
foreach ($ver in $cudaVersions) {
    $testPath = Join-Path $cudaBase $ver
    if (Test-Path $testPath) {
        Write-Host "Using CUDA: $testPath" -ForegroundColor Gray
        $vcvarsEnv["CUDA_PATH"] = $testPath
        $vcvarsEnv["CUDA_ROOT"] = $testPath
        $vcvarsEnv["CUDA_TOOLKIT_ROOT_DIR"] = $testPath
        # Prepend CUDA to vcvars PATH
        if ($vcvarsEnv["PATH"]) {
            $vcvarsEnv["PATH"] = "$testPath\bin;" + $vcvarsEnv["PATH"]
        }
        break
    }
}

# C runtime flags
$vcvarsEnv["CFLAGS"] = "/MD"
$vcvarsEnv["CXXFLAGS"] = "/MD"
Write-Host "C runtime: /MD (dynamic)" -ForegroundColor Gray

# ============================================================================
# Run Tauri in vcvars environment
# ============================================================================
Write-Host "`nStarting Tauri..." -ForegroundColor Cyan

try {
    # Build environment string for cmd
    $envString = ($vcvarsEnv.GetEnumerator() | ForEach-Object { "set `"$($_.Key)=$($_.Value)`"" }) -join " && "
    
    # Run tauri via cmd with vcvars environment
    $tauriArgs = $TauriArgs -join " "
    & cmd /c "$envString && npx tauri $tauriArgs"
}
finally {
    if ($viteJob) {
        Write-Host "`nStopping Vite..." -ForegroundColor Gray
        Stop-Job $viteJob -ErrorAction SilentlyContinue
        Remove-Job $viteJob -ErrorAction SilentlyContinue
    }
}

exit $LASTEXITCODE

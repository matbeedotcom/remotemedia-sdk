#!/usr/bin/env pwsh
# build-package.ps1
# PowerShell script for creating .rmpkg packages (cross-platform: Windows/macOS/Linux)
#
# Usage:
#   .\scripts\build-package.ps1 <manifest-path> <output-path> [options]
#
# Examples:
#   .\scripts\build-package.ps1 browser-demo\examples\calculator.rmpkg.json calculator.rmpkg
#   .\scripts\build-package.ps1 browser-demo\examples\text-processor.rmpkg.json text-processor.rmpkg -Optimize

[CmdletBinding()]
param(
    [Parameter(Position=0, Mandatory=$false)]
    [string]$Manifest,

    [Parameter(Position=1, Mandatory=$false)]
    [string]$Output,

    [Parameter(Mandatory=$false)]
    [switch]$Optimize,

    [Parameter(Mandatory=$false)]
    [switch]$SkipBuild,

    [Parameter(Mandatory=$false)]
    [switch]$Help
)

function Show-Usage {
    Write-Host ""
    Write-Host "Usage: build-package.ps1 <manifest-path> <output-path> [options]" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "Arguments:"
    Write-Host "  manifest-path    Path to manifest JSON file (required)"
    Write-Host "  output-path      Output .rmpkg file path (required)"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -Optimize        Run wasm-opt to reduce binary size (~30-40% reduction)"
    Write-Host "  -SkipBuild       Skip WASM build, use existing binary"
    Write-Host "  -Verbose         Show detailed build output"
    Write-Host "  -Help            Show this help message"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  .\scripts\build-package.ps1 browser-demo\examples\calculator.rmpkg.json calculator.rmpkg" -ForegroundColor Gray
    Write-Host "  .\scripts\build-package.ps1 browser-demo\examples\text-processor.rmpkg.json text-processor.rmpkg -Optimize" -ForegroundColor Gray
    Write-Host "  .\scripts\build-package.ps1 examples\custom.json my-pipeline.rmpkg -SkipBuild" -ForegroundColor Gray
    Write-Host ""
}

function Write-Step {
    param([string]$Message)
    Write-Host $Message -ForegroundColor Blue
}

function Write-Success {
    param([string]$Message)
    Write-Host "  ✓ $Message" -ForegroundColor Green
}

function Write-Warning2 {
    param([string]$Message)
    Write-Host "  ⚠ $Message" -ForegroundColor Yellow
}

function Write-Failure {
    param([string]$Message)
    Write-Host "  ✗ $Message" -ForegroundColor Red
}

# Show help if requested or missing arguments
if ($Help -or -not $Manifest -or -not $Output) {
    Show-Usage
    exit $(if ($Help) { 0 } else { 1 })
}

# Validate manifest file exists
if (-not (Test-Path $Manifest)) {
    Write-Failure ('Manifest file not found: {0}' -f $Manifest)
    exit 1
}

# Get script and project directories
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $ScriptDir

# Display header
Write-Host ''
Write-Host '=======================================================' -ForegroundColor Blue
Write-Host '  RemoteMedia Package Builder' -ForegroundColor Blue
Write-Host '=======================================================' -ForegroundColor Blue
Write-Host ''
Write-Host '  Manifest: ' -NoNewline
Write-Host $Manifest -ForegroundColor Green
Write-Host '  Output:   ' -NoNewline
Write-Host $Output -ForegroundColor Green
Write-Host '  Optimize: ' -NoNewline
Write-Host $(if ($Optimize) { 'Yes' } else { 'No' }) -ForegroundColor $(if ($Optimize) { 'Green' } else { 'Yellow' })
Write-Host ''

# Step 1: Build WASM runtime
$WasmPath = Join-Path $ProjectRoot 'runtime\target\wasm32-wasip1\release\pipeline_executor_wasm.wasm'

if (-not $SkipBuild) {
    Write-Step '[1/4] Building WASM runtime...'

    # Check if cargo is installed
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Failure 'cargo not found. Please install Rust from https://rustup.rs/'
        exit 1
    }

    # Check if wasm32-wasip1 target is installed
    $installedTargets = rustup target list --installed
    if ($installedTargets -notcontains 'wasm32-wasip1') {
        Write-Host '  Installing wasm32-wasip1 target...' -ForegroundColor Yellow
        rustup target add wasm32-wasip1 | Out-Null
    }

    # Build WASM
    Push-Location (Join-Path $ProjectRoot 'runtime')

    try {
        if ($VerbosePreference -eq 'Continue') {
            cargo build --target wasm32-wasip1 `
                --bin pipeline_executor_wasm `
                --no-default-features `
                --features wasm `
                --release
        } else {
            cargo build --target wasm32-wasip1 `
                --bin pipeline_executor_wasm `
                --no-default-features `
                --features wasm `
                --release `
                --quiet 2>&1 | Out-Null

            if ($LASTEXITCODE -ne 0) {
                Write-Failure 'WASM build failed. Run with -Verbose for details.'
                exit 1
            }
        }
    } finally {
        Pop-Location
    }

    $WasmSize = (Get-Item $WasmPath).Length
    $WasmSizeMB = [math]::Round($WasmSize / 1MB, 2)
    Write-Success ('WASM binary built ({0} MB)' -f $WasmSizeMB)
} else {
    Write-Step '[1/4] Skipping WASM build (using existing binary)...'

    if (-not (Test-Path $WasmPath)) {
        Write-Failure ('WASM binary not found: {0}' -f $WasmPath)
        Write-Warning2 'Run without -SkipBuild to build the WASM binary'
        exit 1
    }

    $WasmSize = (Get-Item $WasmPath).Length
    $WasmSizeMB = [math]::Round($WasmSize / 1MB, 2)
    Write-Success ('Using existing WASM binary ({0} MB)' -f $WasmSizeMB)
}

# Step 2: Optimize WASM (optional)
$FinalWasmPath = $WasmPath

if ($Optimize) {
    Write-Step '[2/4] Optimizing WASM binary...'

    # Check if wasm-opt is installed
    if (-not (Get-Command wasm-opt -ErrorAction SilentlyContinue)) {
        Write-Warning2 'wasm-opt not found. Install Binaryen for optimization:'
        Write-Host '    npm install -g binaryen' -ForegroundColor Yellow
        Write-Host '    Or download from: https://github.com/WebAssembly/binaryen/releases' -ForegroundColor Yellow
        Write-Warning2 'Skipping optimization...'
    } else {
        $OptimizedWasm = Join-Path $ProjectRoot 'runtime\target\wasm32-wasip1\release\pipeline_executor_wasm.optimized.wasm'

        if ($VerbosePreference -eq 'Continue') {
            wasm-opt -O3 -o $OptimizedWasm $WasmPath
        } else {
            wasm-opt -O3 -o $OptimizedWasm $WasmPath 2>&1 | Out-Null
        }

        $OptimizedSize = (Get-Item $OptimizedWasm).Length
        $OptimizedSizeMB = [math]::Round($OptimizedSize / 1MB, 2)
        $Reduction = [math]::Round(100 - ($OptimizedSize * 100 / $WasmSize), 1)

        Write-Success ('Optimized: {0} MB -> {1} MB ({2}% reduction)' -f $WasmSizeMB, $OptimizedSizeMB, $Reduction)

        $FinalWasmPath = $OptimizedWasm
    }
} else {
    Write-Step '[2/4] Skipping optimization (use -Optimize to enable)'
}

# Step 3: Create package
Write-Step '[3/4] Creating .rmpkg package...'

# Check if Node.js is installed
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Failure 'node not found. Please install Node.js from https://nodejs.org/'
    exit 1
}

# Check if npm dependencies are installed
$BrowserDemoDir = Join-Path $ProjectRoot 'browser-demo'
$NodeModules = Join-Path $BrowserDemoDir 'node_modules'

if (-not (Test-Path $NodeModules)) {
    Write-Host '  Installing npm dependencies...' -ForegroundColor Yellow
    Push-Location $BrowserDemoDir
    npm install --silent | Out-Null
    Pop-Location
}

# Run packaging script
Push-Location $BrowserDemoDir

try {
    $ManifestFullPath = Join-Path $ProjectRoot $Manifest
    $OutputFullPath = Join-Path $ProjectRoot $Output

    if ($VerbosePreference -eq 'Continue') {
        node scripts\create-package.js `
            --manifest $ManifestFullPath `
            --wasm $FinalWasmPath `
            --output $OutputFullPath
    } else {
        $PackageOutput = node scripts\create-package.js `
            --manifest $ManifestFullPath `
            --wasm $FinalWasmPath `
            --output $OutputFullPath 2>&1

        if ($LASTEXITCODE -ne 0) {
            Write-Failure 'Package creation failed. Run with -Verbose for details.'
            Write-Host $PackageOutput
            exit 1
        }
        Write-Success 'Package created'
    }
} finally {
    Pop-Location
}

# Step 4: Validate package
Write-Step '[4/4] Validating package...'

Push-Location $BrowserDemoDir

try {
    $OutputFullPath = Join-Path $ProjectRoot $Output

    if ($VerbosePreference -eq 'Continue') {
        node scripts\test-package.js $OutputFullPath
    } else {
        $ValidationOutput = node scripts\test-package.js $OutputFullPath 2>&1

        if ($ValidationOutput -match 'VALIDATION PASSED') {
            Write-Success 'Package validation passed'

            # Extract key info
            if ($ValidationOutput -match 'Package file loaded \(([^)]+)\)') {
                $PackageSize = $Matches[1]
                Write-Success ('Package size: {0}' -f $PackageSize)
            }

            if ($ValidationOutput -match 'Nodes: (\d+)') {
                $NodeCount = $Matches[1]
                if ($ValidationOutput -match 'Node types: ([^\r\n]+)') {
                    $NodeTypes = $Matches[1]
                    Write-Success ('Nodes: {0} ({1})' -f $NodeCount, $NodeTypes)
                }
            }
        } else {
            Write-Failure 'Package validation failed'
            Write-Host $ValidationOutput
            exit 1
        }
    }
} finally {
    Pop-Location
}

# Success summary
Write-Host ''
Write-Host '=======================================================' -ForegroundColor Green
Write-Host '  Package created successfully!' -ForegroundColor Green
Write-Host '=======================================================' -ForegroundColor Green
Write-Host ''
Write-Host '  Output: ' -NoNewline
Write-Host $Output -ForegroundColor Green
Write-Host ''
Write-Host 'Next steps:'
Write-Host '  1. Test locally:  ' -NoNewline
Write-Host 'cd browser-demo; npm run dev' -ForegroundColor Blue
Write-Host '  2. Upload package at http://localhost:5173'
Write-Host '  3. Click Run Pipeline to execute'
Write-Host ''

@echo off
REM build-package.bat
REM Windows batch script for creating .rmpkg packages
REM
REM Usage:
REM   scripts\build-package.bat <manifest-path> <output-path> [options]
REM
REM Examples:
REM   scripts\build-package.bat browser-demo\examples\calculator.rmpkg.json calculator.rmpkg
REM   scripts\build-package.bat browser-demo\examples\text-processor.rmpkg.json text-processor.rmpkg --optimize

setlocal enabledelayedexpansion

REM Parse arguments
set MANIFEST=
set OUTPUT=
set OPTIMIZE=false
set SKIP_BUILD=false
set VERBOSE=false
set SHOW_HELP=false

:parse_args
if "%~1"=="" goto args_done
if /I "%~1"=="--help" set SHOW_HELP=true & goto args_done
if /I "%~1"=="-h" set SHOW_HELP=true & goto args_done
if /I "%~1"=="--optimize" set OPTIMIZE=true & shift & goto parse_args
if /I "%~1"=="--skip-build" set SKIP_BUILD=true & shift & goto parse_args
if /I "%~1"=="--verbose" set VERBOSE=true & shift & goto parse_args
if /I "%~1"=="-v" set VERBOSE=true & shift & goto parse_args
if "!MANIFEST!"=="" (
  set MANIFEST=%~1
  shift
  goto parse_args
)
if "!OUTPUT!"=="" (
  set OUTPUT=%~1
  shift
  goto parse_args
)
echo [ERROR] Unknown argument: %~1
goto show_usage

:args_done

if "%SHOW_HELP%"=="true" goto show_usage

if "!MANIFEST!"=="" goto show_usage
if "!OUTPUT!"=="" goto show_usage

REM Validate manifest file exists
if not exist "!MANIFEST!" (
  echo [ERROR] Manifest file not found: !MANIFEST!
  exit /b 1
)

REM Get script directory
set SCRIPT_DIR=%~dp0
set PROJECT_ROOT=%SCRIPT_DIR%..
cd /d "%PROJECT_ROOT%"

echo ========================================================
echo   RemoteMedia Package Builder
echo ========================================================
echo.
echo   Manifest: !MANIFEST!
echo   Output:   !OUTPUT!
echo   Optimize: !OPTIMIZE!
echo.

REM Step 1: Build WASM runtime
set WASM_PATH=%PROJECT_ROOT%\runtime\target\wasm32-wasip1\release\pipeline_executor_wasm.wasm

if "!SKIP_BUILD!"=="false" (
  echo [1/4] Building WASM runtime...

  REM Check if cargo is installed
  where cargo >nul 2>nul
  if errorlevel 1 (
    echo [ERROR] cargo not found. Please install Rust from https://rustup.rs/
    exit /b 1
  )

  REM Check if wasm32-wasip1 target is installed
  rustup target list --installed | findstr /C:"wasm32-wasip1" >nul
  if errorlevel 1 (
    echo [INFO] Installing wasm32-wasip1 target...
    rustup target add wasm32-wasip1
  )

  REM Build WASM
  cd /d "%PROJECT_ROOT%\runtime"

  if "!VERBOSE!"=="true" (
    cargo build --target wasm32-wasip1 --bin pipeline_executor_wasm --no-default-features --features wasm --release
  ) else (
    cargo build --target wasm32-wasip1 --bin pipeline_executor_wasm --no-default-features --features wasm --release --quiet >nul 2>nul
    if errorlevel 1 (
      echo [ERROR] WASM build failed. Run with --verbose for details.
      exit /b 1
    )
  )

  cd /d "%PROJECT_ROOT%"

  REM Get WASM size
  for %%A in ("!WASM_PATH!") do set WASM_SIZE=%%~zA
  set /a WASM_SIZE_MB=!WASM_SIZE! / 1024 / 1024
  echo   [OK] WASM binary built (!WASM_SIZE_MB! MB)
) else (
  echo [1/4] Skipping WASM build (using existing binary)...

  if not exist "!WASM_PATH!" (
    echo [ERROR] WASM binary not found: !WASM_PATH!
    echo Run without --skip-build to build the WASM binary
    exit /b 1
  )

  for %%A in ("!WASM_PATH!") do set WASM_SIZE=%%~zA
  set /a WASM_SIZE_MB=!WASM_SIZE! / 1024 / 1024
  echo   [OK] Using existing WASM binary (!WASM_SIZE_MB! MB)
)

REM Step 2: Optimize WASM (optional)
set FINAL_WASM_PATH=!WASM_PATH!

if "!OPTIMIZE!"=="true" (
  echo [2/4] Optimizing WASM binary...

  REM Check if wasm-opt is installed
  where wasm-opt >nul 2>nul
  if errorlevel 1 (
    echo [WARNING] wasm-opt not found. Install Binaryen for optimization:
    echo   npm install -g binaryen
    echo   Or download from: https://github.com/WebAssembly/binaryen/releases
    echo Skipping optimization...
  ) else (
    set OPTIMIZED_WASM=%PROJECT_ROOT%\runtime\target\wasm32-wasip1\release\pipeline_executor_wasm.optimized.wasm

    if "!VERBOSE!"=="true" (
      wasm-opt -O3 -o "!OPTIMIZED_WASM!" "!WASM_PATH!"
    ) else (
      wasm-opt -O3 -o "!OPTIMIZED_WASM!" "!WASM_PATH!" >nul 2>nul
    )

    for %%A in ("!OPTIMIZED_WASM!") do set OPTIMIZED_SIZE=%%~zA
    set /a OPTIMIZED_SIZE_MB=!OPTIMIZED_SIZE! / 1024 / 1024
    set /a REDUCTION=100 - (!OPTIMIZED_SIZE! * 100 / !WASM_SIZE!)

    echo   [OK] Optimized: !WASM_SIZE_MB! MB -^> !OPTIMIZED_SIZE_MB! MB (!REDUCTION!%% reduction)

    set FINAL_WASM_PATH=!OPTIMIZED_WASM!
  )
) else (
  echo [2/4] Skipping optimization (use --optimize to enable)
)

REM Step 3: Create package
echo [3/4] Creating .rmpkg package...

REM Check if Node.js is installed
where node >nul 2>nul
if errorlevel 1 (
  echo [ERROR] node not found. Please install Node.js from https://nodejs.org/
  exit /b 1
)

REM Check if npm dependencies are installed
if not exist "%PROJECT_ROOT%\browser-demo\node_modules" (
  echo [INFO] Installing npm dependencies...
  cd /d "%PROJECT_ROOT%\browser-demo"
  npm install --silent
  cd /d "%PROJECT_ROOT%"
)

REM Run packaging script
cd /d "%PROJECT_ROOT%\browser-demo"

if "!VERBOSE!"=="true" (
  node scripts\create-package.js --manifest "%PROJECT_ROOT%\!MANIFEST!" --wasm "!FINAL_WASM_PATH!" --output "%PROJECT_ROOT%\!OUTPUT!"
) else (
  node scripts\create-package.js --manifest "%PROJECT_ROOT%\!MANIFEST!" --wasm "!FINAL_WASM_PATH!" --output "%PROJECT_ROOT%\!OUTPUT!" 2>&1 | findstr /C:"OK" /C:"Error" /C:"Warning" >nul
  if errorlevel 1 (
    echo [ERROR] Package creation failed. Run with --verbose for details.
    exit /b 1
  )
  echo   [OK] Package created
)

cd /d "%PROJECT_ROOT%"

REM Step 4: Validate package
echo [4/4] Validating package...

cd /d "%PROJECT_ROOT%\browser-demo"

if "!VERBOSE!"=="true" (
  node scripts\test-package.js "%PROJECT_ROOT%\!OUTPUT!"
) else (
  node scripts\test-package.js "%PROJECT_ROOT%\!OUTPUT!" 2>&1 | findstr /C:"VALIDATION PASSED" >nul
  if errorlevel 1 (
    echo [ERROR] Package validation failed. Run with --verbose for details.
    node scripts\test-package.js "%PROJECT_ROOT%\!OUTPUT!"
    exit /b 1
  )
  echo   [OK] Package validation passed

  REM Get package size
  for %%A in ("%PROJECT_ROOT%\!OUTPUT!") do set PKG_SIZE=%%~zA
  set /a PKG_SIZE_MB=!PKG_SIZE! / 1024 / 1024
  echo   [OK] Package size: !PKG_SIZE_MB! MB
)

cd /d "%PROJECT_ROOT%"

REM Success summary
echo.
echo ========================================================
echo   Package created successfully!
echo ========================================================
echo.
echo   Output: !OUTPUT!
echo.
echo Next steps:
echo   1. Test locally:  cd browser-demo ^&^& npm run dev
echo   2. Upload package at http://localhost:5173
echo   3. Click 'Run Pipeline' to execute
echo.

exit /b 0

:show_usage
echo.
echo Usage: %~nx0 ^<manifest-path^> ^<output-path^> [options]
echo.
echo Arguments:
echo   manifest-path    Path to manifest JSON file (required)
echo   output-path      Output .rmpkg file path (required)
echo.
echo Options:
echo   --optimize       Run wasm-opt to reduce binary size (~30-40%% reduction)
echo   --skip-build     Skip WASM build, use existing binary
echo   --verbose        Show detailed build output
echo   --help           Show this help message
echo.
echo Examples:
echo   %~nx0 browser-demo\examples\calculator.rmpkg.json calculator.rmpkg
echo   %~nx0 browser-demo\examples\text-processor.rmpkg.json text-processor.rmpkg --optimize
echo   %~nx0 examples\custom.json my-pipeline.rmpkg --skip-build
echo.
exit /b 1

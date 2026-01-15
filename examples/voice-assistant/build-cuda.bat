@echo off
REM Build script for Voice Assistant with CUDA support
REM This script sets up the Visual Studio and CUDA environment, then builds

REM Initialize Visual Studio environment
call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 (
    echo Failed to initialize Visual Studio environment
    exit /b 1
)

REM Set CUDA 12.6 environment
set CUDA_PATH=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6
set CUDA_ROOT=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6
set CUDA_TOOLKIT_ROOT_DIR=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6
set PATH=C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\v12.6\bin;%PATH%

REM Set C runtime flags to avoid linker mismatch
set CFLAGS=/MD
set CXXFLAGS=/MD

REM Verify CUDA version
echo.
echo CUDA version:
nvcc --version
echo.

REM Change to src-tauri directory and build
cd /d "%~dp0src-tauri"
cargo build --release

if errorlevel 1 (
    echo Build failed!
    exit /b 1
)

echo.
echo Build successful! Binary at: target\release\voice-assistant.exe

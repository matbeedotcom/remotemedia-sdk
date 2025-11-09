#!/bin/bash
set -e

echo "=== Whisper WASM/WASI Build Script for Ubuntu/WSL ==="
echo ""

# Configuration
WASI_SDK_VERSION="27.0"
WASI_SDK_ARCH="x86_64-linux"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WASI_SDK_DIR="${SCRIPT_DIR}/wasi-sdk-${WASI_SDK_VERSION}-${WASI_SDK_ARCH}"
WHISPER_DIR="${SCRIPT_DIR}/whisper.cpp"
BUILD_DIR="${WHISPER_DIR}/build-wasi"

# Step 1: Install dependencies
echo "Step 1: Installing dependencies..."
if ! command -v cmake &> /dev/null; then
    echo "Installing cmake..."
    sudo apt-get update
    sudo apt-get install -y cmake build-essential git curl
else
    echo "cmake already installed"
fi

# Step 2: Download wasi-sdk if not present
if [ ! -d "${WASI_SDK_DIR}" ]; then
    echo ""
    echo "Step 2: Downloading wasi-sdk ${WASI_SDK_VERSION}..."
    WASI_SDK_URL="https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-${WASI_SDK_VERSION%%.*}/wasi-sdk-${WASI_SDK_VERSION}-${WASI_SDK_ARCH}.tar.gz"
    echo "URL: ${WASI_SDK_URL}"

    curl -L "${WASI_SDK_URL}" -o wasi-sdk.tar.gz
    tar -xzf wasi-sdk.tar.gz
    rm wasi-sdk.tar.gz
    echo "wasi-sdk extracted to ${WASI_SDK_DIR}"
else
    echo ""
    echo "Step 2: wasi-sdk already present at ${WASI_SDK_DIR}"
fi

# Step 3: Clone whisper.cpp if not present
if [ ! -d "${WHISPER_DIR}" ]; then
    echo ""
    echo "Step 3: Cloning whisper.cpp..."
    git clone https://github.com/ggerganov/whisper.cpp.git "${WHISPER_DIR}"
else
    echo ""
    echo "Step 3: whisper.cpp already present"
fi

# Step 4: Patch whisper.cpp for WASI
echo ""
echo "Step 4: Patching whisper.cpp for WASI builds..."

# Patch ggml/CMakeLists.txt to make threads optional
GGML_CMAKE="${WHISPER_DIR}/ggml/CMakeLists.txt"
if ! grep -q "Make threads optional for WASI" "${GGML_CMAKE}"; then
    echo "Patching ${GGML_CMAKE}..."

    # Find the line with find_package(Threads REQUIRED)
    sed -i '/set(THREADS_PREFER_PTHREAD_FLAG ON)/a\
\
# Make threads optional for WASI\
if(NOT CMAKE_SYSTEM_NAME STREQUAL "WASI")\
    find_package(Threads REQUIRED)\
else()\
    # WASI with wasi-threads should have pthread support\
    set(CMAKE_THREAD_LIBS_INIT "-pthread")\
    set(CMAKE_HAVE_THREADS_LIBRARY 1)\
    set(CMAKE_USE_PTHREADS_INIT 1)\
    set(Threads_FOUND TRUE)\
endif()' "${GGML_CMAKE}"

    # Comment out the original find_package line
    sed -i 's/^find_package(Threads REQUIRED)/# find_package(Threads REQUIRED) # Moved to conditional block above/' "${GGML_CMAKE}"

    echo "✓ Patched ggml/CMakeLists.txt"
else
    echo "✓ ggml/CMakeLists.txt already patched"
fi

# Patch ggml/src/CMakeLists.txt to conditionally link threads
GGML_SRC_CMAKE="${WHISPER_DIR}/ggml/src/CMakeLists.txt"
if ! grep -q "Only link threads if not building for WASI" "${GGML_SRC_CMAKE}"; then
    echo "Patching ${GGML_SRC_CMAKE}..."

    # Replace the Threads::Threads linking
    sed -i 's/target_link_libraries(ggml-base PRIVATE Threads::Threads)/# Only link threads if not building for WASI (wasi-threads provides pthread via -pthread flag)\
if(NOT CMAKE_SYSTEM_NAME STREQUAL "WASI")\
    target_link_libraries(ggml-base PRIVATE Threads::Threads)\
else()\
    # For WASI with wasi-threads, pthread is provided via compiler flags\
    target_compile_options(ggml-base PRIVATE -pthread)\
    target_link_options(ggml-base PRIVATE -pthread)\
endif()/' "${GGML_SRC_CMAKE}"

    echo "✓ Patched ggml/src/CMakeLists.txt"
else
    echo "✓ ggml/src/CMakeLists.txt already patched"
fi

# Step 5: Configure CMake with wasi-sdk
echo ""
echo "Step 5: Configuring CMake with wasi-sdk..."

# Clean build directory
rm -rf "${BUILD_DIR}"
mkdir -p "${BUILD_DIR}"
cd "${BUILD_DIR}"

# Use wasi-sdk's pthread toolchain
TOOLCHAIN_FILE="${WASI_SDK_DIR}/share/cmake/wasi-sdk-pthread.cmake"

if [ ! -f "${TOOLCHAIN_FILE}" ]; then
    echo "ERROR: Toolchain file not found: ${TOOLCHAIN_FILE}"
    exit 1
fi

echo "Using toolchain: ${TOOLCHAIN_FILE}"
echo "Build directory: ${BUILD_DIR}"

cmake .. \
    -DCMAKE_TOOLCHAIN_FILE="${TOOLCHAIN_FILE}" \
    -DWASI_SDK_PREFIX="${WASI_SDK_DIR}" \
    -DGGML_OPENMP=OFF \
    -DWHISPER_BUILD_TESTS=OFF \
    -DWHISPER_BUILD_EXAMPLES=OFF \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_VERBOSE_MAKEFILE=ON \
    -DCMAKE_C_FLAGS="-D_WASI_EMULATED_SIGNAL -D_WASI_EMULATED_PROCESS_CLOCKS" \
    -DCMAKE_CXX_FLAGS="-D_WASI_EMULATED_SIGNAL -D_WASI_EMULATED_PROCESS_CLOCKS" \
    -DCMAKE_EXE_LINKER_FLAGS="-lwasi-emulated-signal -lwasi-emulated-process-clocks"

# Step 6: Build
echo ""
echo "Step 6: Building whisper for WASI..."
cmake --build . --target whisper --config Release -j$(nproc)

# Step 7: Verify output
echo ""
echo "Step 7: Verifying build output..."

if [ -f "libwhisper.a" ]; then
    echo "✓ Success! libwhisper.a built"
    ls -lh libwhisper.a
    file libwhisper.a
elif [ -f "src/libwhisper.a" ]; then
    echo "✓ Success! libwhisper.a built"
    ls -lh src/libwhisper.a
    file src/libwhisper.a
else
    echo "✗ Error: libwhisper.a not found"
    echo "Contents of build directory:"
    find . -name "*.a" -o -name "*.wasm"
    exit 1
fi

echo ""
echo "=== Build Complete ==="
echo ""
echo "Next steps:"
echo "1. Build Rust runtime with whisper-wasm feature:"
echo "   cd runtime"
echo "   export CMAKE_TOOLCHAIN_FILE_wasm32_wasip1=\"${TOOLCHAIN_FILE}\""
echo "   export BINDGEN_EXTRA_CLANG_ARGS=\"--sysroot=${WASI_SDK_DIR}/share/wasi-sysroot\""
echo "   cargo build --target wasm32-wasip1 --features whisper-wasm --no-default-features"
echo ""
echo "2. Test with wasmtime:"
echo "   wasmtime run --dir=. target/wasm32-wasip1/debug/pipeline_executor_wasm.wasm"

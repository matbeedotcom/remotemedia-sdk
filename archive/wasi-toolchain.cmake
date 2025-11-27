# CMake toolchain file for WASI

set(CMAKE_SYSTEM_NAME WASI)
set(CMAKE_SYSTEM_VERSION 1)
set(CMAKE_SYSTEM_PROCESSOR wasm32)
set(triple wasm32-wasi)

set(WASI_SDK_PREFIX "C:/Users/mail/dev/personal/remotemedia-sdk/wasi-sdk-27.0-x86_64-windows")

set(CMAKE_C_COMPILER ${WASI_SDK_PREFIX}/bin/clang.exe)
set(CMAKE_CXX_COMPILER ${WASI_SDK_PREFIX}/bin/clang++.exe)
set(CMAKE_ASM_COMPILER ${WASI_SDK_PREFIX}/bin/clang.exe)
set(CMAKE_AR ${WASI_SDK_PREFIX}/bin/llvm-ar.exe)
set(CMAKE_RANLIB ${WASI_SDK_PREFIX}/bin/llvm-ranlib.exe)
set(CMAKE_C_COMPILER_TARGET ${triple})
set(CMAKE_CXX_COMPILER_TARGET ${triple})
set(CMAKE_ASM_COMPILER_TARGET ${triple})

# Skip compiler checks
set(CMAKE_C_COMPILER_WORKS 1)
set(CMAKE_CXX_COMPILER_WORKS 1)
set(CMAKE_ASM_COMPILER_WORKS 1)

# Don't look for executables in target directories
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
# Look for includes/libraries in target directories
set(CMAKE_FIND_ROOT_PATH_MODE_LIBRARY ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_INCLUDE ONLY)
set(CMAKE_FIND_ROOT_PATH_MODE_PACKAGE ONLY)

# Use wasi-threads sysroot for pthread support
set(WASI_SYSROOT "${WASI_SDK_PREFIX}/share/wasi-sysroot")
set(CMAKE_C_FLAGS "${CMAKE_C_FLAGS} --sysroot=${WASI_SYSROOT} -pthread")
set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} --sysroot=${WASI_SYSROOT} -pthread")

# Enable threads with wasi-threads
set(THREADS_PREFER_PTHREAD_FLAG ON)

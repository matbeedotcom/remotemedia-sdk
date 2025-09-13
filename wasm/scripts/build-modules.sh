#!/bin/bash
# Build script for WASM modules
# This script compiles Rust and C source code into WebAssembly modules

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Directories
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SRC_DIR="$PROJECT_ROOT/src/wasm-modules"
OUTPUT_DIR="$PROJECT_ROOT/modules"
RUST_SRC="$SRC_DIR/rust"
C_SRC="$SRC_DIR/c"

echo -e "${GREEN}🔧 Building WASM modules for RemoteMedia Pipeline${NC}"
echo "Project root: $PROJECT_ROOT"

# Create output directories
mkdir -p "$OUTPUT_DIR/audio"
mkdir -p "$OUTPUT_DIR/vision"
mkdir -p "$OUTPUT_DIR/text"

# Check for required tools
check_tool() {
    if ! command -v $1 &> /dev/null; then
        echo -e "${RED}Error: $1 is not installed${NC}"
        echo "Please install $1 to build WASM modules"
        exit 1
    fi
}

echo -e "${YELLOW}Checking build tools...${NC}"
check_tool rustc
check_tool cargo
check_tool clang

# Check for WASM targets
if ! rustup target list --installed | grep -q "wasm32-wasi"; then
    echo -e "${YELLOW}Installing wasm32-wasi target...${NC}"
    rustup target add wasm32-wasi
fi

# Function to build Rust WASM modules
build_rust_module() {
    local module_name=$1
    local output_category=$2
    local src_path="$RUST_SRC/$module_name"
    
    if [ ! -d "$src_path" ]; then
        echo -e "${YELLOW}Warning: Source not found for $module_name, creating template...${NC}"
        create_rust_template "$module_name" "$output_category"
        src_path="$RUST_SRC/$module_name"
    fi
    
    echo -e "${GREEN}Building Rust module: $module_name${NC}"
    
    cd "$src_path"
    
    # Build with optimizations for size and performance
    RUSTFLAGS="-C link-arg=-s" cargo build --target wasm32-wasi --release
    
    # Copy to output directory
    cp "target/wasm32-wasi/release/${module_name}.wasm" "$OUTPUT_DIR/$output_category/"
    
    # Optimize with wasm-opt if available
    if command -v wasm-opt &> /dev/null; then
        echo "  Optimizing with wasm-opt..."
        wasm-opt -O3 -o "$OUTPUT_DIR/$output_category/${module_name}_opt.wasm" \
                 "$OUTPUT_DIR/$output_category/${module_name}.wasm"
        mv "$OUTPUT_DIR/$output_category/${module_name}_opt.wasm" \
           "$OUTPUT_DIR/$output_category/${module_name}.wasm"
    fi
    
    # Get file size
    size=$(du -h "$OUTPUT_DIR/$output_category/${module_name}.wasm" | cut -f1)
    echo -e "  ✅ Built: ${module_name}.wasm (${size})"
    
    cd - > /dev/null
}

# Function to build C WASM modules
build_c_module() {
    local module_name=$1
    local output_category=$2
    local src_file="$C_SRC/${module_name}.c"
    
    if [ ! -f "$src_file" ]; then
        echo -e "${YELLOW}Warning: Source not found for $module_name, creating template...${NC}"
        create_c_template "$module_name" "$output_category"
        src_file="$C_SRC/${module_name}.c"
    fi
    
    echo -e "${GREEN}Building C module: $module_name${NC}"
    
    # Compile with clang for WASI target
    clang --target=wasm32-wasi \
          -O3 \
          -nostdlib \
          -Wl,--no-entry \
          -Wl,--export-all \
          -Wl,--allow-undefined \
          -o "$OUTPUT_DIR/$output_category/${module_name}.wasm" \
          "$src_file"
    
    # Get file size
    size=$(du -h "$OUTPUT_DIR/$output_category/${module_name}.wasm" | cut -f1)
    echo -e "  ✅ Built: ${module_name}.wasm (${size})"
}

# Function to create Rust template
create_rust_template() {
    local module_name=$1
    local category=$2
    local module_path="$RUST_SRC/$module_name"
    
    mkdir -p "$module_path/src"
    
    # Create Cargo.toml
    cat > "$module_path/Cargo.toml" << EOF
[package]
name = "$module_name"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]

[profile.release]
lto = true
opt-level = "s"
panic = "abort"
EOF
    
    # Create lib.rs template
    cat > "$module_path/src/lib.rs" << 'EOF'
use std::slice;
use std::mem;

#[no_mangle]
pub extern "C" fn initialize(session_id_ptr: *const u8, session_id_len: usize) -> i32 {
    // Initialize session
    1
}

#[no_mangle]
pub extern "C" fn process(
    input_ptr: *const u8,
    input_len: usize,
    output_ptr: *mut u8,
    output_len: *mut usize
) -> i32 {
    // Process input and write to output
    let input = unsafe { slice::from_raw_parts(input_ptr, input_len) };
    
    // Simple echo for template
    let output = unsafe { slice::from_raw_parts_mut(output_ptr, input_len) };
    output.copy_from_slice(input);
    
    unsafe { *output_len = input_len; }
    
    0 // Success
}

#[no_mangle]
pub extern "C" fn cleanup() -> i32 {
    // Cleanup resources
    0
}
EOF
    
    echo -e "  Created Rust template for $module_name"
}

# Function to create C template
create_c_template() {
    local module_name=$1
    local category=$2
    local src_file="$C_SRC/${module_name}.c"
    
    mkdir -p "$C_SRC"
    
    cat > "$src_file" << 'EOF'
#include <stdint.h>
#include <string.h>

// Export functions
__attribute__((export_name("initialize")))
int32_t initialize(const uint8_t* session_id, size_t session_len) {
    // Initialize session
    return 1;
}

__attribute__((export_name("process")))
int32_t process(const uint8_t* input, size_t input_len,
                uint8_t* output, size_t* output_len) {
    // Simple echo for template
    memcpy(output, input, input_len);
    *output_len = input_len;
    return 0;
}

__attribute__((export_name("cleanup")))
int32_t cleanup() {
    // Cleanup resources
    return 0;
}
EOF
    
    echo -e "  Created C template for $module_name"
}

# Build Audio Processing Modules
echo -e "\n${GREEN}📊 Building Audio Processing Modules${NC}"
build_rust_module "fast_vad" "audio"
build_rust_module "audio_preprocess" "audio"
build_rust_module "audio_enhance" "audio"
build_rust_module "simple_asr" "audio"
build_c_module "simple_tts" "audio"

# Build Vision Processing Modules
echo -e "\n${GREEN}👁️ Building Vision Processing Modules${NC}"
build_rust_module "image_preprocess" "vision"
build_c_module "basic_ocr" "vision"

# Build Text Processing Modules
echo -e "\n${GREEN}📝 Building Text Processing Modules${NC}"
build_rust_module "text_classify" "text"
build_c_module "simple_tokenizer" "text"

# Summary
echo -e "\n${GREEN}🎉 Build completed successfully!${NC}"
echo -e "Modules available in: ${OUTPUT_DIR}/"
echo -e "\nAudio modules:"
ls -lh "$OUTPUT_DIR/audio/" | grep ".wasm"
echo -e "\nVision modules:"
ls -lh "$OUTPUT_DIR/vision/" | grep ".wasm" || echo "  No vision modules built"
echo -e "\nText modules:"
ls -lh "$OUTPUT_DIR/text/" | grep ".wasm" || echo "  No text modules built"

# Total size
total_size=$(du -sh "$OUTPUT_DIR" | cut -f1)
echo -e "\n${GREEN}Total size: ${total_size}${NC}"

echo -e "\n${GREEN}Next steps:${NC}"
echo "1. Test modules: ./scripts/test-integration.sh"
echo "2. Run example: python wasm/examples/hybrid-speech-pipeline.py"
echo "3. Deploy: ./scripts/deploy.sh"
// Build script for remotemedia-runtime
// Used for generating protobuf code and other build-time tasks

fn main() {
    // Phase 1: No protobuf generation yet
    // Will be added in Phase 1.4 for FFI layer
    println!("cargo:rerun-if-changed=build.rs");
}

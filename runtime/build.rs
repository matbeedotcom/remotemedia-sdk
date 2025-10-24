// Build script for remotemedia-runtime
// Handles protobuf generation and WASM-specific static library configuration

fn main() {
    // WASM-specific: Configure static Python libraries for wasm32-wasi target
    #[cfg(target_family = "wasm")]
    configure_wasm_libs();

    // Standard build tasks
    println!("cargo:rerun-if-changed=build.rs");
}

/// Configure static libraries for WASM build (wasm32-wasi target)
#[cfg(target_family = "wasm")]
fn configure_wasm_libs() {
    use wlr_libpy::bld_cfg::configure_static_libs;

    // Download and configure libpython3.12.a + wasi-sysroot + clang builtins
    configure_static_libs()
        .expect("Failed to configure static libraries for WASM")
        .emit_link_flags();

    println!("cargo:warning=Configured WASM static libraries (libpython3.12.a + wasi-sysroot)");
}

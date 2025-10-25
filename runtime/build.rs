// Build script for remotemedia-runtime
// Handles protobuf generation and WASM-specific static library configuration

fn main() {
    // Check if we're building for WASM target
    // CARGO_CFG_TARGET_FAMILY is set by cargo during build
    let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();

    if target_family == "wasm" {
        println!("cargo:warning=Detected WASM target, configuring static Python libraries");
        configure_wasm_libs();
    }

    // Standard build tasks
    println!("cargo:rerun-if-changed=build.rs");
}

/// Configure static libraries for WASM build (wasm32-wasi target)
fn configure_wasm_libs() {
    use wlr_libpy::bld_cfg::configure_static_libs;

    // Download and configure libpython3.12.a + wasi-sysroot + clang builtins
    match configure_static_libs() {
        Ok(config) => {
            config.emit_link_flags();
            println!("cargo:warning=Successfully configured WASM static libraries");
        }
        Err(e) => {
            panic!("Failed to configure static libraries for WASM: {}", e);
        }
    }
}

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

    // Compile protocol buffers if grpc-transport feature is enabled
    #[cfg(feature = "grpc-transport")]
    compile_protos();

    // Standard build tasks
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=protos/");
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

/// Compile protocol buffers for gRPC service
#[cfg(feature = "grpc-transport")]
fn compile_protos() {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true) // Enable client generation for testing
        .out_dir("src/grpc_service/generated") // Output to dedicated directory
        .compile_protos(
            &[
                "protos/common.proto",
                "protos/execution.proto",
                "protos/streaming.proto",
            ],
            &["protos/"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile protos: {}", e));

    println!("cargo:warning=Successfully compiled protocol buffers");
}

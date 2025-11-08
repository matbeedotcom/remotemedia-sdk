// Build script for gRPC code generation

#[cfg(feature = "grpc-signaling")]
fn main() {
    compile_protos();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=protos/");
}

#[cfg(not(feature = "grpc-signaling"))]
fn main() {
    // No-op when grpc-signaling feature is disabled
}

#[cfg(feature = "grpc-signaling")]
fn compile_protos() {
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir("src/generated")
        .compile_protos(&["protos/webrtc_signaling.proto"], &["protos/"])
        .unwrap_or_else(|e| panic!("Failed to compile protos: {}", e));

    println!("cargo:warning=Successfully compiled protocol buffers");
}

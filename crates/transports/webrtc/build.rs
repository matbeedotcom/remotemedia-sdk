// Build script for gRPC code generation

#[cfg(feature = "grpc-signaling")]
fn main() {
    // Use vendored protoc from protobuf-src
    std::env::set_var("PROTOC", protobuf_src::protoc());

    compile_protos();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../../proto/");
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
        .compile_protos(
            &["../../../proto/webrtc_signaling.proto", "../../../proto/common.proto"],
            &["../../../proto/"],
        )
        .unwrap_or_else(|e| panic!("Failed to compile protos: {}", e));

    println!("cargo:warning=Successfully compiled protocol buffers");
}

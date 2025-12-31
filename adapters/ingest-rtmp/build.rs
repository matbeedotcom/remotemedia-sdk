// Build script for remotemedia-ingest-rtmp
// Links against ac-ffmpeg's wrapper library for struct accessor functions

use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Find the ac-ffmpeg build output directory by searching in the target directory
    // The structure is: target/{debug|release}/build/ac-ffmpeg-*/out/libffwrapper.a
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let out_path = PathBuf::from(&out_dir);

    // Walk up to find the build directory
    // OUT_DIR is like: target/debug/build/ingest-rtmp-xxx/out
    // We need:         target/debug/build/ac-ffmpeg-xxx/out
    let build_dir = out_path
        .parent() // ingest-rtmp-xxx/out -> ingest-rtmp-xxx
        .and_then(|p| p.parent()) // ingest-rtmp-xxx -> build
        .expect("Could not find build directory");

    // Search for ac-ffmpeg's output directory
    let mut ffwrapper_dir = None;
    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("ac-ffmpeg-") {
                let candidate = entry.path().join("out").join("libffwrapper.a");
                if candidate.exists() {
                    ffwrapper_dir = Some(entry.path().join("out"));
                    break;
                }
            }
        }
    }

    if let Some(dir) = ffwrapper_dir {
        println!("cargo:rustc-link-search=native={}", dir.display());
        println!("cargo:rustc-link-lib=static=ffwrapper");
        println!("cargo:warning=Found ffwrapper at: {}", dir.display());
    } else {
        // Fallback: just emit the link directive and hope ac-ffmpeg's search path is used
        println!("cargo:warning=Could not find libffwrapper.a, relying on ac-ffmpeg's search path");
        println!("cargo:rustc-link-lib=static=ffwrapper");
    }
}

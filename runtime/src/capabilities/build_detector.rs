//! Build-time capability detection via compilation trials
//!
//! Simple approach: Try to compile the node for each target.
//! If compilation succeeds → node supports that target.
//! If compilation fails → node doesn't support that target.
//!
//! No guessing, no heuristics, just empirical compilation results.

use super::{NodeCapabilities, ExecutionPlacement};
use std::process::Command;
use std::path::Path;

/// Detect capabilities by attempting to build for each target
///
/// Tries targets in order from most restrictive to least:
/// 1. wasm32-unknown-unknown (browser) → most restrictive
/// 2. wasm32-wasip1 (server WASM) → moderate restrictions
/// 3. wasm32-wasip1-threads (server WASM + threads) → fewer restrictions
/// 4. native (no restrictions)
pub fn detect_via_compilation(node_source: &Path) -> NodeCapabilities {
    let mut caps = NodeCapabilities::default();

    // Trial 1: Pure WASM (most portable)
    if try_compile_browser_wasm(node_source) {
        caps.supports_browser = true;
        caps.supports_wasm = true;
        caps.requires_threads = false;
        caps.requires_native_libs = false;
        caps.requires_gpu = false;
        caps.placement = ExecutionPlacement::Anywhere;

        println!("✓ Node compiles to pure WASM (wasm32-unknown-unknown)");
        println!("  Can run: browser, server WASM, native");
        return caps; // If it works as pure WASM, it works everywhere
    }

    // Trial 2: WASI WASM (needs WASI APIs)
    if try_compile_wasi(node_source) {
        caps.supports_browser = false; // Failed pure WASM, needs WASI
        caps.supports_wasm = true;
        caps.requires_threads = false;
        caps.requires_native_libs = false; // Compiles to WASM, but needs WASI APIs
        caps.placement = ExecutionPlacement::RequiresWasi;

        println!("✓ Node compiles to WASI WASM (wasm32-wasip1)");
        println!("  Can run: server WASM, native (not browser)");
        return caps;
    }

    // Trial 3: WASI WASM with threads
    if try_compile_wasi_threads(node_source) {
        caps.supports_browser = false;
        caps.supports_wasm = true;
        caps.requires_threads = true; // Needs threads since regular WASI failed
        caps.requires_native_libs = false;
        caps.placement = ExecutionPlacement::RequiresWasi;

        println!("✓ Node compiles to WASI WASM+threads (wasm32-wasip1-threads)");
        println!("  Can run: server WASM, native (not browser)");
        return caps;
    }

    // Trial 4: Native only
    if try_compile_native(node_source) {
        caps.supports_browser = false;
        caps.supports_wasm = false; // Failed all WASM targets
        caps.requires_native_libs = true;
        caps.placement = ExecutionPlacement::RequiresNative;

        // Infer why it failed WASM by checking build output
        caps.requires_threads = check_build_output_for_threads();
        caps.requires_gpu = check_build_output_for_gpu();

        println!("✓ Node compiles natively only (can't build for WASM)");
        return caps;
    }

    // All builds failed
    println!("✗ Node failed to compile for any target");
    caps.supports_browser = false;
    caps.supports_wasm = false;
    caps.placement = ExecutionPlacement::RequiresRemote;
    caps
}

/// Try to compile for browser (wasm32-unknown-unknown)
fn try_compile_browser_wasm(source: &Path) -> bool {
    let output = Command::new("cargo")
        .args(&[
            "build",
            "--target", "wasm32-unknown-unknown",
            "--release",
            "--manifest-path", source.to_str().unwrap(),
        ])
        .output();

    match output {
        Ok(result) => result.status.success(),
        Err(_) => false,
    }
}

/// Try to compile for server WASM (wasm32-wasip1)
fn try_compile_wasi(source: &Path) -> bool {
    let output = Command::new("cargo")
        .args(&[
            "build",
            "--target", "wasm32-wasip1",
            "--release",
            "--manifest-path", source.to_str().unwrap(),
            "--no-default-features",
        ])
        .output();

    match output {
        Ok(result) => result.status.success(),
        Err(_) => false,
    }
}

/// Try to compile for server WASM with threads
fn try_compile_wasi_threads(source: &Path) -> bool {
    // Set environment for wasi-threads
    let output = Command::new("cargo")
        .args(&[
            "build",
            "--target", "wasm32-wasip1-threads",
            "--release",
            "--manifest-path", source.to_str().unwrap(),
            "--no-default-features",
        ])
        .env("RUSTFLAGS", "-C target-feature=+atomics,+bulk-memory,+mutable-globals")
        .output();

    match output {
        Ok(result) => result.status.success(),
        Err(_) => false,
    }
}

/// Try to compile natively
fn try_compile_native(source: &Path) -> bool {
    let output = Command::new("cargo")
        .args(&[
            "build",
            "--release",
            "--manifest-path", source.to_str().unwrap(),
        ])
        .output();

    match output {
        Ok(result) => result.status.success(),
        Err(_) => false,
    }
}

/// Check build output for threading-related errors
fn check_build_output_for_threads() -> bool {
    // In real implementation: parse last build stderr for pthread, threading, etc.
    // For now, return conservative guess
    false
}

/// Check build output for GPU-related errors
fn check_build_output_for_gpu() -> bool {
    // In real implementation: parse last build stderr for cuda, metal, etc.
    false
}

/// Detect capabilities for Python nodes by analyzing imports
///
/// Python nodes can't be compiled, so we analyze their imports:
/// - Pure stdlib imports → browser compatible
/// - C-extension imports (torch, numpy, whisper) → native only
pub fn detect_python_node_capabilities(node_source: &Path) -> NodeCapabilities {
    let mut caps = NodeCapabilities::default();

    // Read Python source
    let source_content = match std::fs::read_to_string(node_source) {
        Ok(content) => content,
        Err(_) => {
            // Can't read source, be conservative - assume needs host
            caps.supports_browser = false;
            caps.placement = ExecutionPlacement::PreferNative;
            return caps;
        }
    };

    // Check for C-extension imports (these won't work in browser)
    let c_extensions = [
        "import numpy", "import torch", "import whisper",
        "import cv2", "import tensorflow", "from scipy",
        "import pandas", "import sklearn",
    ];

    for ext in &c_extensions {
        if source_content.contains(ext) {
            caps.supports_browser = false;
            caps.supports_wasm = false; // Most C-extensions don't compile to WASM
            caps.requires_native_libs = true;
            caps.placement = ExecutionPlacement::RequiresRemote;

            println!("✗ Python node imports C-extension: {}", ext);
            return caps;
        }
    }

    // Check for threading
    if source_content.contains("threading.") || source_content.contains("multiprocessing.") {
        caps.requires_threads = true;
        caps.supports_browser = false; // SharedArrayBuffer issues
    }

    // Check for file I/O
    if source_content.contains("open(") || source_content.contains("file.") {
        caps.supports_browser = false; // File I/O doesn't work in browser
    }

    // Pure Python, stdlib only → can run in Pyodide (browser)
    if caps.supports_browser {
        println!("✓ Python node uses only stdlib (Pyodide-compatible)");
        println!("  Can run: browser (Pyodide), server, native");
        caps.supports_wasm = true;
        caps.placement = ExecutionPlacement::Anywhere;
    } else {
        println!("✓ Python node requires native execution (C-extensions)");
        println!("  Can run: native only");
        caps.placement = ExecutionPlacement::RequiresNative;
    }

    caps
}

/// CLI tool to detect and print node capabilities
pub fn detect_and_print(node_path: &str) -> Result<(), String> {
    let path = Path::new(node_path);

    if !path.exists() {
        return Err(format!("Path does not exist: {}", node_path));
    }

    println!("Detecting capabilities for: {}", node_path);
    println!("Attempting compilation for different targets...\n");

    let caps = if path.extension().and_then(|s| s.to_str()) == Some("py") {
        detect_python_node_capabilities(path)
    } else {
        detect_via_compilation(path)
    };

    println!("\n=== Detected Capabilities ===");
    println!("Supports browser:     {}", caps.supports_browser);
    println!("Supports WASM:        {}", caps.supports_wasm);
    println!("Requires threads:     {}", caps.requires_threads);
    println!("Requires native libs: {}", caps.requires_native_libs);
    println!("Requires GPU:         {}", caps.requires_gpu);
    println!("Placement:            {:?}", caps.placement);
    println!("Est. memory (MB):     {}", caps.estimated_memory_mb);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_pure_python_node() {
        // Create a temporary pure Python node
        let temp_dir = std::env::temp_dir();
        let node_file = temp_dir.join("test_node.py");

        std::fs::write(&node_file, r#"
import json
import sys

class SimpleNode:
    def process(self, data):
        return {"result": data * 2}
"#).unwrap();

        let caps = detect_python_node_capabilities(&node_file);

        assert!(caps.supports_browser, "Pure Python should support browser");
        assert!(caps.supports_wasm, "Pure Python can run in Pyodide");
        assert!(!caps.requires_native_libs, "No C extensions");
    }

    #[test]
    fn test_detect_c_extension_python_node() {
        let temp_dir = std::env::temp_dir();
        let node_file = temp_dir.join("test_node_numpy.py");

        std::fs::write(&node_file, r#"
import numpy as np

class NumpyNode:
    def process(self, data):
        return np.array(data) * 2
"#).unwrap();

        let caps = detect_python_node_capabilities(&node_file);

        assert!(!caps.supports_browser, "Numpy prevents browser execution");
        assert!(!caps.supports_wasm, "Numpy C-extension can't compile to WASM");
        assert!(caps.requires_native_libs, "Numpy is native library");
    }
}

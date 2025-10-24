//! WASM binary for executing RemoteMedia pipelines in browser
//!
//! This binary embeds CPython via libpython3.12.a and provides
//! a WASI Command interface for running pipelines.
//!
//! Entry point: `_start` (WASI Command standard)
//! Input: Manifest JSON via stdin
//! Output: Execution results via stdout

use pyo3::prelude::*;
use std::io::{self, Read};

fn main() -> PyResult<()> {
    // Initialize PyO3 for WASM environment
    pyo3::prepare_freethreaded_python();

    println!("RemoteMedia WASM Runtime");
    println!("Python version: {}", get_python_version()?);

    // Read manifest from stdin
    let manifest_json = read_stdin()?;

    println!("Received manifest ({} bytes)", manifest_json.len());

    // Execute pipeline (will be implemented in next phase)
    Python::with_gil(|py| {
        execute_pipeline_wasm(py, &manifest_json)
    })
}

/// Get Python version from embedded CPython
fn get_python_version() -> PyResult<String> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let version: String = sys.getattr("version")?.extract()?;
        Ok(version)
    })
}

/// Execute pipeline from manifest JSON (stub for now)
fn execute_pipeline_wasm(py: Python<'_>, manifest_json: &str) -> PyResult<()> {
    // Parse manifest (stub)
    println!("Parsing manifest...");

    // For now, just echo back a success message
    let result = serde_json::json!({
        "status": "success",
        "message": "WASM runtime initialized successfully",
        "manifest_size": manifest_json.len(),
        "python_initialized": true
    });

    // Write results to stdout
    println!("\n{}", serde_json::to_string_pretty(&result).unwrap());

    Ok(())
}

/// Read input from stdin
fn read_stdin() -> PyResult<String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyIOError, _>(
            format!("Failed to read stdin: {}", e)
        ))?;
    Ok(buffer)
}

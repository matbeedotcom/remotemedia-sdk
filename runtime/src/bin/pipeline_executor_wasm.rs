//! WASM binary for executing RemoteMedia pipelines in browser
//!
//! This binary embeds CPython via libpython3.12.a and provides
//! a WASI Command interface for running pipelines.
//!
//! Entry point: `_start` (WASI Command standard)
//! Input: Manifest JSON via stdin
//! Output: Execution results via stdout

use pyo3::prelude::*;
use remotemedia_runtime::executor::{Executor, ExecutorConfig};
use remotemedia_runtime::manifest::Manifest;
use std::io::{self, Read};

fn main() {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("RemoteMedia WASM Runtime starting");

    // Read manifest from stdin
    let manifest_json = match read_stdin() {
        Ok(json) => json,
        Err(e) => {
            eprintln!("Failed to read stdin: {}", e);
            std::process::exit(1);
        }
    };

    tracing::info!("Received manifest ({} bytes)", manifest_json.len());

    // Execute pipeline
    match execute_pipeline_wasm(&manifest_json) {
        Ok(()) => {
            tracing::info!("Pipeline execution completed successfully");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Pipeline execution failed: {}", e);
            std::process::exit(1);
        }
    }
}

/// Get Python version from embedded CPython
fn get_python_version() -> PyResult<String> {
    Python::with_gil(|py| {
        let sys = py.import("sys")?;
        let version: String = sys.getattr("version")?.extract()?;
        Ok(version)
    })
}

/// Execute pipeline from manifest JSON
fn execute_pipeline_wasm(manifest_json: &str) -> Result<(), String> {
    // Parse input JSON - can be either just manifest or manifest + input_data
    tracing::info!("Parsing input...");
    let input: serde_json::Value =
        serde_json::from_str(manifest_json).map_err(|e| format!("Failed to parse input: {}", e))?;

    // Extract manifest and optional input_data
    let (manifest, input_data) = if input.get("manifest").is_some() {
        // Format: { "manifest": {...}, "input_data": [...] }
        let manifest: Manifest = serde_json::from_value(input["manifest"].clone())
            .map_err(|e| format!("Failed to parse manifest: {}", e))?;
        let input_data = input
            .get("input_data")
            .and_then(|v| v.as_array())
            .map(|arr| arr.clone())
            .unwrap_or_default();
        (manifest, input_data)
    } else {
        // Format: Just the manifest { "version": "v1", ... }
        let manifest: Manifest = serde_json::from_value(input)
            .map_err(|e| format!("Failed to parse manifest: {}", e))?;
        (manifest, vec![])
    };

    tracing::info!(
        "Manifest parsed: {} (version {})",
        manifest.metadata.name,
        manifest.version
    );
    tracing::info!(
        "Pipeline has {} nodes and {} connections",
        manifest.nodes.len(),
        manifest.connections.len()
    );
    tracing::info!("Input data items: {}", input_data.len());

    // Create executor
    let executor = Executor::with_config(ExecutorConfig {
        max_concurrency: 4, // Limit concurrency in WASM
        debug: true,
    });

    // Execute pipeline synchronously (WASM-compatible)
    tracing::info!("Executing pipeline...");
    let result = if !input_data.is_empty() {
        // Execute with provided input data
        executor
            .execute_with_input_sync(&manifest, input_data)
            .map_err(|e| format!("Pipeline execution failed: {}", e))?
    } else {
        // Execute without input (source-based)
        executor
            .execute_sync(&manifest)
            .map_err(|e| format!("Pipeline execution failed: {}", e))?
    };

    // Serialize and output results
    let output_json = serde_json::to_string_pretty(&serde_json::json!({
        "status": result.status,
        "outputs": result.outputs,
        "graph_info": result.graph_info.as_ref().map(|info| serde_json::json!({
            "node_count": info.node_count,
            "source_count": info.source_count,
            "sink_count": info.sink_count,
            "execution_order": info.execution_order,
        }))
    }))
    .map_err(|e| format!("Failed to serialize results: {}", e))?;

    // Write results to stdout
    println!("\n=== PIPELINE RESULTS ===");
    println!("{}", output_json);

    Ok(())
}

/// Read input from stdin
fn read_stdin() -> Result<String, String> {
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .map_err(|e| format!("Failed to read stdin: {}", e))?;
    Ok(buffer)
}

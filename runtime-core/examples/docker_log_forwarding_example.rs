//! Docker log forwarding example
//!
//! Demonstrates how to use the container log forwarding system to forward
//! container stdout/stderr to the tracing infrastructure.
//!
//! Run with:
//! ```bash
//! RUST_LOG=debug cargo run --example docker_log_forwarding_example --features docker
//! ```

use remotemedia_runtime_core::python::multiprocess::docker_support::{
    DockerNodeConfig, DockerSupport, LogForwardingConfig, LogLevel,
};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("Docker Log Forwarding Example");
    println!("==============================\n");

    // Create Docker support
    let docker_support = DockerSupport::new().await?;
    println!("✓ Docker support initialized\n");

    // Create a simple test container configuration
    let config = DockerNodeConfig {
        python_version: "3.11".to_string(),
        base_image: Some("python:3.11-slim".to_string()),
        system_packages: vec![],
        python_packages: vec![],
        memory_mb: 512,
        cpu_cores: 1.0,
        gpu_devices: vec![],
        shm_size_mb: 256,
        env_vars: HashMap::new(),
        volumes: vec![],
        security: Default::default(),
    };

    // Create and start a container
    println!("Creating test container...");
    let container_id = docker_support
        .create_container("test_log_node", "test_session", &config)
        .await?;

    println!("✓ Container created: {}", container_id);

    // Start the container with a simple Python script that generates logs
    println!("Starting container with log generation script...\n");

    // First, we need to execute a command in the container
    use bollard::exec::{CreateExecOptions, StartExecOptions};

    // Create exec instance to run a Python script that generates various log levels
    let exec_config = CreateExecOptions {
        cmd: Some(vec![
            "python3",
            "-c",
            r#"
import sys
import time
import json

# Generate logs with different formats
print("INFO: Container started and ready", flush=True)
print("DEBUG: Initializing test script", flush=True)
time.sleep(1)

# JSON formatted log
json_log = {"level": "info", "message": "JSON formatted log entry", "timestamp": "2024-01-01T00:00:00Z"}
print(json.dumps(json_log), flush=True)
time.sleep(1)

# Warning message
print("WARN: This is a warning message", flush=True)
time.sleep(1)

# Error to stderr
print("ERROR: This is an error message", file=sys.stderr, flush=True)
time.sleep(1)

# More JSON logs
for i in range(3):
    log = {"level": "debug", "message": f"Processing item {i}", "item_id": i}
    print(json.dumps(log), flush=True)
    time.sleep(0.5)

print("INFO: Test script completed", flush=True)
            "#,
        ]),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        ..Default::default()
    };

    // Start the container first
    docker_support.start_container(&container_id).await?;
    println!("✓ Container started\n");

    // Configure log forwarding with custom settings
    let log_config = LogForwardingConfig {
        enabled: true,
        buffer_size: 8192,          // 8KB buffer
        min_level: LogLevel::Debug, // Forward Debug and above
        parse_json: true,           // Parse JSON logs
        include_timestamps: true,
    };

    // Start log forwarding
    println!("Starting log forwarding (watching container logs)...\n");
    let shutdown_tx = docker_support
        .forward_container_logs(&container_id, Some("test_log_node"), log_config)
        .await?;

    // Now execute the log-generating script
    let exec = docker_support
        .docker_client()
        .create_exec(&container_id, exec_config)
        .await?;

    // Start the exec and collect output
    let exec_id = exec.id;
    let start_config = StartExecOptions {
        detach: false,
        ..Default::default()
    };

    use bollard::exec::StartExecResults;
    use futures::StreamExt;

    let exec_result = docker_support
        .docker_client()
        .start_exec(&exec_id, Some(start_config))
        .await?;

    println!("=== Container Output (will be forwarded to tracing) ===\n");

    if let StartExecResults::Attached { mut output, .. } = exec_result {
        while let Some(result) = output.next().await {
            if let Ok(msg) = result {
                // The logs will be automatically forwarded to tracing by our forwarding task
                // We can also print them here for demonstration
                print!("{}", msg);
            }
        }
    }

    // Wait a bit more to ensure all logs are forwarded
    println!("\n\n=== Waiting for log forwarding to complete ===\n");
    sleep(Duration::from_secs(2)).await;

    // Cleanup
    println!("\n--- Cleanup ---\n");

    // Stop log forwarding
    let _ = shutdown_tx.send(true);
    println!("✓ Log forwarding stopped");

    // Stop and remove container
    docker_support
        .stop_container(&container_id, Duration::from_secs(5))
        .await?;
    println!("✓ Container stopped");

    docker_support.remove_container(&container_id, true).await?;
    println!("✓ Container removed");

    println!("\nExample completed successfully!");
    println!("\nNote: Check the logs above - container logs should be forwarded");
    println!("with appropriate log levels (DEBUG, INFO, WARN, ERROR) and metadata.");

    Ok(())
}

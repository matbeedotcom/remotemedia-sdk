//! Integration tests for the `remotemedia serve` command
//! Tests pipeline server startup

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::time::Duration;
use tempfile::TempDir;

/// Test: Serve command starts and binds to port
#[test]
fn test_serve_starts() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("server.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
metadata:
  name: "server-pipeline"
nodes:
  - id: echo
    node_type: EchoNode
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("serve")
        .arg(manifest_path.to_str().unwrap())
        .arg("--port")
        .arg("0") // Use any available port
        .timeout(Duration::from_secs(2));

    // Should start and be interruptible
    cmd.assert().interrupted();
}

/// Test: Serve command fails if port in use
#[test]
#[ignore = "Requires port binding"]
fn test_serve_port_in_use() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("server.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: echo
    node_type: EchoNode
connections: []
"#,
    )
    .unwrap();

    // Bind to a port first
    let _listener = std::net::TcpListener::bind("127.0.0.1:18080").unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("serve")
        .arg(manifest_path.to_str().unwrap())
        .arg("--port")
        .arg("18080");

    cmd.assert()
        .failure()
        .code(2) // Port already in use
        .stderr(predicate::str::contains("in use").or(predicate::str::contains("bind")));
}

/// Test: Serve command accepts transport option
#[test]
fn test_serve_transport_grpc() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("server.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: echo
    node_type: EchoNode
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("serve")
        .arg(manifest_path.to_str().unwrap())
        .arg("--transport")
        .arg("grpc")
        .arg("--port")
        .arg("0")
        .timeout(Duration::from_secs(2));

    cmd.assert().interrupted();
}

/// Test: Serve command accepts transport option for HTTP
#[test]
fn test_serve_transport_http() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("server.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: echo
    node_type: EchoNode
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("serve")
        .arg(manifest_path.to_str().unwrap())
        .arg("--transport")
        .arg("http")
        .arg("--port")
        .arg("0")
        .timeout(Duration::from_secs(2));

    cmd.assert().interrupted();
}

/// Test: Serve command with invalid manifest fails
#[test]
fn test_serve_invalid_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("invalid.yaml");

    fs::write(&manifest_path, "invalid yaml content").unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("serve").arg(manifest_path.to_str().unwrap());

    cmd.assert().failure().code(1);
}

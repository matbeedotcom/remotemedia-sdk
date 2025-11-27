//! Integration tests for the `remotemedia remote` subcommands
//! Tests remote pipeline execution

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test: remote run requires server
#[test]
fn test_remote_run_requires_server() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("test.yaml");

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
    cmd.arg("remote")
        .arg("run")
        .arg(manifest_path.to_str().unwrap());

    // Should fail because no server is configured
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("server").or(predicate::str::contains("--server")));
}

/// Test: remote run with server option
#[test]
fn test_remote_run_with_server() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("test.yaml");

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
    cmd.arg("remote")
        .arg("run")
        .arg(manifest_path.to_str().unwrap())
        .arg("--server")
        .arg("grpc://localhost:50051");

    // Will fail to connect but should parse arguments correctly
    cmd.assert()
        .failure()
        .stderr(
            predicate::str::contains("connection")
                .or(predicate::str::contains("connect"))
                .or(predicate::str::contains("refused")),
        );
}

/// Test: remote run with named pipeline
#[test]
fn test_remote_run_named_pipeline() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("remote")
        .arg("run")
        .arg("--server")
        .arg("grpc://localhost:50051")
        .arg("--pipeline")
        .arg("whisper-transcribe");

    // Will fail to connect but should parse arguments correctly
    cmd.assert().failure();
}

/// Test: remote stream requires server
#[test]
fn test_remote_stream_requires_server() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("remote")
        .arg("stream")
        .arg("--pipeline")
        .arg("voice-assistant");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("server").or(predicate::str::contains("--server")));
}

/// Test: remote stream with server
#[test]
fn test_remote_stream_with_server() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("remote")
        .arg("stream")
        .arg("--server")
        .arg("grpc://localhost:50051")
        .arg("--pipeline")
        .arg("voice-assistant")
        .timeout(std::time::Duration::from_secs(2));

    // Will fail to connect but should parse arguments
    cmd.assert().failure();
}

/// Test: servers list shows configured servers
#[test]
fn test_servers_list() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("servers").arg("list");

    cmd.assert().success();
}

/// Test: servers add creates server entry
#[test]
fn test_servers_add() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("servers")
        .arg("add")
        .arg("test-server")
        .arg("grpc://test.example.com:50051");

    cmd.assert().success();
}

/// Test: servers add with invalid URL fails
#[test]
fn test_servers_add_invalid_url() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("servers")
        .arg("add")
        .arg("bad-server")
        .arg("not-a-valid-url");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("URL").or(predicate::str::contains("scheme")));
}

/// Test: servers remove
#[test]
fn test_servers_remove() {
    // First add a server
    let mut add_cmd = Command::cargo_bin("remotemedia").unwrap();
    add_cmd
        .arg("servers")
        .arg("add")
        .arg("to-remove")
        .arg("grpc://temp.example.com:50051")
        .assert()
        .success();

    // Then remove it
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("servers").arg("remove").arg("to-remove");

    cmd.assert().success();
}

/// Test: servers remove unknown server
#[test]
fn test_servers_remove_unknown() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("servers").arg("remove").arg("nonexistent-server");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

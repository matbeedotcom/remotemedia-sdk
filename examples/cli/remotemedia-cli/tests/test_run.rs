//! Integration tests for the `remotemedia run` command
//! Tests unary pipeline execution

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test: Run command with valid manifest and input produces output
#[test]
fn test_run_with_valid_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("test.yaml");
    let input_path = temp_dir.path().join("input.txt");
    let output_path = temp_dir.path().join("output.txt");

    // Create a simple test manifest
    fs::write(
        &manifest_path,
        r#"
version: "v1"
metadata:
  name: "test-pipeline"
nodes:
  - id: echo
    node_type: EchoNode
connections: []
"#,
    )
    .unwrap();

    // Create test input
    fs::write(&input_path, "test input data").unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("run")
        .arg(manifest_path.to_str().unwrap())
        .arg("--input")
        .arg(input_path.to_str().unwrap())
        .arg("--output")
        .arg(output_path.to_str().unwrap());

    cmd.assert().success();

    // Verify output was created
    assert!(output_path.exists(), "Output file should be created");
}

/// Test: Run command fails with invalid manifest
#[test]
fn test_run_with_invalid_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("invalid.yaml");

    // Create invalid manifest
    fs::write(&manifest_path, "invalid: yaml: content:").unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("run").arg(manifest_path.to_str().unwrap());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Invalid manifest"));
}

/// Test: Run command fails when input file not found
#[test]
fn test_run_input_not_found() {
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
    cmd.arg("run")
        .arg(manifest_path.to_str().unwrap())
        .arg("--input")
        .arg("/nonexistent/input.wav");

    cmd.assert()
        .failure()
        .code(3); // Exit code 3 = input file not found
}

/// Test: Run command respects timeout
#[test]
fn test_run_with_timeout() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("test.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: slow
    node_type: SlowNode
    params:
      delay_ms: 5000
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("run")
        .arg(manifest_path.to_str().unwrap())
        .arg("--timeout")
        .arg("1s");

    cmd.assert()
        .failure()
        .code(4); // Exit code 4 = timeout
}

/// Test: Run command outputs JSON when requested
#[test]
fn test_run_json_output() {
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
    cmd.arg("run")
        .arg(manifest_path.to_str().unwrap())
        .arg("--output-format")
        .arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::starts_with("{"));
}

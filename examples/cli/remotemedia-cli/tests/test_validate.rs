//! Integration tests for the `remotemedia validate` command
//! Tests manifest validation without execution

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test: Validate command passes for valid manifest
#[test]
fn test_validate_valid_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("valid.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
metadata:
  name: "valid-pipeline"
  description: "A valid test pipeline"
nodes:
  - id: node1
    node_type: EchoNode
  - id: node2
    node_type: EchoNode
connections:
  - from: node1
    to: node2
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate").arg(manifest_path.to_str().unwrap());

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("valid").or(predicate::str::contains("Valid")));
}

/// Test: Validate command fails for invalid YAML
#[test]
fn test_validate_invalid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("invalid.yaml");

    fs::write(&manifest_path, "not: valid: yaml: [").unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate").arg(manifest_path.to_str().unwrap());

    cmd.assert().failure().code(1);
}

/// Test: Validate command fails for missing version
#[test]
fn test_validate_missing_version() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("no_version.yaml");

    fs::write(
        &manifest_path,
        r#"
nodes:
  - id: test
    node_type: EchoNode
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate").arg(manifest_path.to_str().unwrap());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("version"));
}

/// Test: Validate command detects circular dependencies
#[test]
fn test_validate_circular_dependency() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("circular.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: a
    node_type: EchoNode
  - id: b
    node_type: EchoNode
connections:
  - from: a
    to: b
  - from: b
    to: a
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate").arg(manifest_path.to_str().unwrap());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("circular").or(predicate::str::contains("cycle")));
}

/// Test: Validate command detects duplicate node IDs
#[test]
fn test_validate_duplicate_node_ids() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("duplicate.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: same_id
    node_type: EchoNode
  - id: same_id
    node_type: EchoNode
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate").arg(manifest_path.to_str().unwrap());

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("duplicate").or(predicate::str::contains("unique")));
}

/// Test: Validate with --check-nodes verifies node types exist
#[test]
fn test_validate_check_nodes() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("unknown_node.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: unknown
    node_type: NonExistentNode
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate")
        .arg(manifest_path.to_str().unwrap())
        .arg("--check-nodes");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("unknown")));
}

/// Test: Validate command with JSON output
#[test]
fn test_validate_json_output() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("valid.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: test
    node_type: EchoNode
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("validate")
        .arg(manifest_path.to_str().unwrap())
        .arg("--output-format")
        .arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::starts_with("{"));
}

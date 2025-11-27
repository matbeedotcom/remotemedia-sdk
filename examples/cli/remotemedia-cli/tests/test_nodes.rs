//! Integration tests for the `remotemedia nodes` subcommand
//! Tests node listing and information display

use assert_cmd::Command;
use predicates::prelude::*;

/// Test: nodes list shows available nodes
#[test]
fn test_nodes_list() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes").arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("NODE TYPE").or(predicate::str::contains("node_type")));
}

/// Test: nodes list with filter
#[test]
fn test_nodes_list_filter() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes").arg("list").arg("--filter").arg("VAD");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("VAD").or(predicate::str::contains("vad")));
}

/// Test: nodes list with category filter
#[test]
fn test_nodes_list_category() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes").arg("list").arg("--category").arg("audio");

    cmd.assert().success();
}

/// Test: nodes info shows node details
#[test]
fn test_nodes_info() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes").arg("info").arg("SileroVADNode");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("SileroVADNode"))
        .stdout(predicate::str::contains("Parameters").or(predicate::str::contains("params")));
}

/// Test: nodes info for unknown node
#[test]
fn test_nodes_info_unknown() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes").arg("info").arg("NonExistentNode");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found").or(predicate::str::contains("unknown")));
}

/// Test: nodes list with JSON output
#[test]
fn test_nodes_list_json() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes")
        .arg("list")
        .arg("--output-format")
        .arg("json");

    cmd.assert()
        .success()
        .stdout(predicate::str::starts_with("[").or(predicate::str::starts_with("{")));
}

/// Test: nodes list with table output
#[test]
fn test_nodes_list_table() {
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("nodes")
        .arg("list")
        .arg("--output-format")
        .arg("table");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("│").or(predicate::str::contains("|")));
}

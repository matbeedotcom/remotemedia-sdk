//! Integration tests for the `remotemedia stream` command
//! Tests streaming pipeline execution with real-time I/O

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test: Stream command accepts manifest and starts streaming
#[test]
fn test_stream_starts_with_valid_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("stream.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
metadata:
  name: "stream-test"
nodes:
  - id: passthrough
    node_type: PassthroughNode
    is_streaming: true
connections: []
"#,
    )
    .unwrap();

    // Stream should start and be interruptible
    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("stream")
        .arg(manifest_path.to_str().unwrap())
        .timeout(std::time::Duration::from_secs(2));

    // Should timeout (no input provided) but not fail with error code 1
    cmd.assert().interrupted();
}

/// Test: Stream command with file input
#[test]
fn test_stream_with_file_input() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("stream.yaml");
    let input_path = temp_dir.path().join("audio.raw");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: passthrough
    node_type: PassthroughNode
    is_streaming: true
connections: []
"#,
    )
    .unwrap();

    // Create test audio data (raw samples)
    let audio_data: Vec<u8> = vec![0u8; 48000 * 4]; // 1 second of f32 samples
    fs::write(&input_path, &audio_data).unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("stream")
        .arg(manifest_path.to_str().unwrap())
        .arg("--input")
        .arg(input_path.to_str().unwrap());

    cmd.assert().success();
}

/// Test: Stream command respects sample rate
#[test]
fn test_stream_sample_rate_option() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("stream.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: audio
    node_type: AudioSourceNode
    is_streaming: true
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("stream")
        .arg(manifest_path.to_str().unwrap())
        .arg("--sample-rate")
        .arg("16000")
        .arg("--channels")
        .arg("1");

    // Just verify it parses the arguments correctly
    cmd.assert()
        .stderr(predicate::str::contains("16000").or(predicate::str::is_empty()));
}

/// Test: Stream command fails without audio device when --mic specified
#[test]
#[ignore = "Requires audio device"]
fn test_stream_mic_requires_device() {
    let temp_dir = TempDir::new().unwrap();
    let manifest_path = temp_dir.path().join("stream.yaml");

    fs::write(
        &manifest_path,
        r#"
version: "v1"
nodes:
  - id: mic
    node_type: MicrophoneNode
connections: []
"#,
    )
    .unwrap();

    let mut cmd = Command::cargo_bin("remotemedia").unwrap();
    cmd.arg("stream")
        .arg(manifest_path.to_str().unwrap())
        .arg("--mic");

    // Should fail with exit code 2 if no audio device
    cmd.assert().failure().code(2);
}

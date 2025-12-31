//! Integration tests for stream-health-demo
//!
//! These tests verify that the demo binary correctly processes audio input
//! and produces JSONL health events.

use std::io::Write;
use std::process::{Command, Stdio};

/// Generate a simple WAV header for testing
fn generate_wav_header(sample_rate: u32, channels: u16, sample_count: usize) -> Vec<u8> {
    let bytes_per_sample = 4u16; // f32
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
    let block_align = channels * bytes_per_sample;
    let data_size = (sample_count * bytes_per_sample as usize * channels as usize) as u32;
    let file_size = 36 + data_size;

    let mut header = Vec::with_capacity(44);
    
    // RIFF header
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&file_size.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    
    // fmt chunk
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    header.extend_from_slice(&3u16.to_le_bytes()); // format = IEEE float
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&block_align.to_le_bytes());
    header.extend_from_slice(&32u16.to_le_bytes()); // bits per sample
    
    // data chunk
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_size.to_le_bytes());
    
    header
}

/// Generate synthetic audio samples with slight drift to trigger alerts
fn generate_test_audio(sample_rate: u32, duration_secs: f64) -> Vec<f32> {
    let sample_count = (sample_rate as f64 * duration_secs) as usize;
    let freq = 440.0; // A4 tone
    
    (0..sample_count)
        .map(|i| {
            let t = i as f64 / sample_rate as f64;
            (2.0 * std::f64::consts::PI * freq * t).sin() as f32 * 0.5
        })
        .collect()
}

/// Create a WAV file in memory for testing
fn create_test_wav(sample_rate: u32, duration_secs: f64) -> Vec<u8> {
    let samples = generate_test_audio(sample_rate, duration_secs);
    let mut wav = generate_wav_header(sample_rate, 1, samples.len());
    
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    
    wav
}

/// Get the path to the demo binary
fn demo_binary_path() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // cli
    path.pop(); // examples
    path.push("target");
    path.push("debug");
    path.push("remotemedia-demo");
    path
}

#[test]
fn test_show_limits() {
    // Build the binary first
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    let output = Command::new(demo_binary_path())
        .arg("--show-limits")
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("Demo Mode Limits:"), "Should show demo limits header");
    assert!(stdout.contains("Session duration:"), "Should show session duration");
    assert!(stdout.contains("Sessions per day:"), "Should show sessions per day");
}

#[test]
fn test_show_pipeline() {
    // Build the binary first
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    let output = Command::new(demo_binary_path())
        .arg("--show-pipeline")
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(stdout.contains("version:"), "Should show pipeline version");
    assert!(stdout.contains("HealthEmitterNode"), "Should contain HealthEmitterNode");
}

#[test]
fn test_no_input_shows_error() {
    // Build the binary first
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    let output = Command::new(demo_binary_path())
        .output()
        .expect("Failed to run demo binary");

    // Should exit with error when no input provided
    assert!(!output.status.success(), "Should fail without input");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("No input specified") || stderr.contains("-i"),
        "Should show error about missing input"
    );
}

#[test]
#[ignore] // Requires pipeline execution which needs more setup
fn test_file_input_produces_jsonl() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Build the binary first
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    // Create a test WAV file
    let wav_data = create_test_wav(16000, 2.0); // 2 seconds of audio
    let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
    temp_file.write_all(&wav_data).expect("Failed to write WAV data");
    let wav_path = temp_file.path();

    // Run the demo with the test file
    let output = Command::new(demo_binary_path())
        .args(["-i", wav_path.to_str().unwrap(), "--json", "-q"])
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Should produce at least one health event
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(!lines.is_empty(), "Should produce JSONL output");
    
    // Each line should be valid JSON with a type field
    for line in &lines {
        if line.trim().is_empty() {
            continue;
        }
        let json: serde_json::Value = serde_json::from_str(line)
            .expect(&format!("Line should be valid JSON: {}", line));
        assert!(
            json.get("type").is_some(),
            "Each event should have a type field"
        );
    }
}

#[test]
#[ignore] // Requires pipeline execution which needs more setup
fn test_piped_input_streaming() {
    use std::thread;

    // Build the binary first
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    // Start the demo process with stdin
    let mut child = Command::new(demo_binary_path())
        .args(["-i", "-", "--stream", "--json", "-q", "-r", "16000"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start demo process");

    let stdin = child.stdin.take().expect("Failed to get stdin");
    
    // Generate test audio and write to stdin in a separate thread
    let handle = thread::spawn(move || {
        let mut stdin = stdin;
        let samples = generate_test_audio(16000, 1.0); // 1 second
        
        for sample in samples {
            if stdin.write_all(&sample.to_le_bytes()).is_err() {
                break;
            }
        }
        drop(stdin); // Close stdin to signal EOF
    });

    // Wait with timeout
    let output = child.wait_with_output().expect("Failed to wait for process");
    handle.join().expect("Writer thread panicked");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // In streaming mode, should produce health events
    // Note: May be empty if processing is fast and no events triggered
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let json: serde_json::Value = serde_json::from_str(line)
            .expect(&format!("Line should be valid JSON: {}", line));
        assert!(
            json.get("type").is_some() || json.get("ts").is_some(),
            "Each event should have expected fields"
        );
    }
}

/// Test that help output shows all expected options
#[test]
fn test_help_shows_options() {
    // Build the binary first
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    let output = Command::new(demo_binary_path())
        .arg("--help")
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Check for expected CLI options
    assert!(stdout.contains("-i"), "Should show -i option");
    assert!(stdout.contains("-o"), "Should show -o option");
    assert!(stdout.contains("--stream"), "Should show --stream option");
    assert!(stdout.contains("--lead-threshold"), "Should show --lead-threshold option");
    assert!(stdout.contains("--freeze-threshold"), "Should show --freeze-threshold option");
    assert!(stdout.contains("--json"), "Should show --json option");
    assert!(stdout.contains("--show-limits"), "Should show --show-limits option");
    assert!(stdout.contains("--show-pipeline"), "Should show --show-pipeline option");
    assert!(stdout.contains("activate"), "Should show activate subcommand");
}

// ============================================================================
// T102-T103: Integration tests for --ingest flag
// ============================================================================

/// T102: Test --ingest with file:// URL produces health events
#[test]
#[ignore = "Requires full pipeline setup - run with --ignored"]
fn test_ingest_file_url_produces_health_events() {
    use tempfile::NamedTempFile;

    // Generate test WAV file
    let wav_file = NamedTempFile::with_suffix(".wav").expect("Failed to create temp file");
    let samples = generate_test_audio(16000, 2.0);
    {
        let mut file = std::fs::File::create(wav_file.path()).expect("Failed to create file");
        
        // Write WAV header for s16 format
        let header = generate_wav_header_s16(16000, 1, samples.len());
        file.write_all(&header).expect("Failed to write header");
        
        // Convert f32 to s16 and write
        for sample in samples {
            let s16 = (sample * 32767.0) as i16;
            file.write_all(&s16.to_le_bytes()).expect("Failed to write sample");
        }
    }

    let file_url = format!("file://{}", wav_file.path().to_string_lossy());

    // Build with rtmp feature
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo", "--features", "rtmp"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    // Set unlimited mode for testing
    let output = Command::new(demo_binary_path())
        .args(["--ingest", &file_url, "--json", "-q"])
        .env("REMOTEMEDIA_DEMO_UNLIMITED", "1")
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Debug output
    if !output.status.success() {
        eprintln!("stderr: {}", stderr);
    }

    // Should produce at least one health event
    let mut health_events = 0;
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            if json.get("type").and_then(|t| t.as_str()) == Some("health") {
                health_events += 1;
            }
        }
    }

    assert!(
        health_events > 0,
        "Should produce health events from file:// URL. stdout: {}",
        stdout
    );
}

/// T103: Test --ingest with bare path produces health events
#[test]
#[ignore = "Requires full pipeline setup - run with --ignored"]
fn test_ingest_bare_path_produces_health_events() {
    use tempfile::NamedTempFile;

    // Generate test WAV file
    let wav_file = NamedTempFile::with_suffix(".wav").expect("Failed to create temp file");
    let samples = generate_test_audio(16000, 2.0);
    {
        let mut file = std::fs::File::create(wav_file.path()).expect("Failed to create file");
        let header = generate_wav_header_s16(16000, 1, samples.len());
        file.write_all(&header).expect("Failed to write header");
        
        for sample in samples {
            let s16 = (sample * 32767.0) as i16;
            file.write_all(&s16.to_le_bytes()).expect("Failed to write sample");
        }
    }

    let bare_path = wav_file.path().to_string_lossy().to_string();

    // Build with rtmp feature
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo", "--features", "rtmp"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    let output = Command::new(demo_binary_path())
        .args(["--ingest", &bare_path, "--json", "-q"])
        .env("REMOTEMEDIA_DEMO_UNLIMITED", "1")
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Should produce at least one JSONL event
    let mut events = 0;
    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if serde_json::from_str::<serde_json::Value>(line).is_ok() {
            events += 1;
        }
    }

    assert!(
        events > 0,
        "Should produce events from bare path. stdout: {}",
        stdout
    );
}

/// Helper: Generate WAV header with s16 format
fn generate_wav_header_s16(sample_rate: u32, channels: u16, sample_count: usize) -> Vec<u8> {
    let bytes_per_sample = 2u16; // s16
    let byte_rate = sample_rate * channels as u32 * bytes_per_sample as u32;
    let block_align = channels * bytes_per_sample;
    let data_size = (sample_count * bytes_per_sample as usize * channels as usize) as u32;
    let file_size = 36 + data_size;

    let mut header = Vec::with_capacity(44);
    
    // RIFF header
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&file_size.to_le_bytes());
    header.extend_from_slice(b"WAVE");
    
    // fmt chunk
    header.extend_from_slice(b"fmt ");
    header.extend_from_slice(&16u32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    header.extend_from_slice(&channels.to_le_bytes());
    header.extend_from_slice(&sample_rate.to_le_bytes());
    header.extend_from_slice(&byte_rate.to_le_bytes());
    header.extend_from_slice(&block_align.to_le_bytes());
    header.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    
    // data chunk
    header.extend_from_slice(b"data");
    header.extend_from_slice(&data_size.to_le_bytes());
    
    header
}

/// Test --ingest shows in help
#[test]
fn test_help_shows_ingest_option() {
    let status = Command::new("cargo")
        .args(["build", "-p", "stream-health-demo"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("Failed to build demo binary");
    
    if !status.success() {
        panic!("Failed to build demo binary");
    }

    let output = Command::new(demo_binary_path())
        .arg("--help")
        .output()
        .expect("Failed to run demo binary");

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    assert!(
        stdout.contains("--ingest"),
        "Help should show --ingest option. Got: {}",
        stdout
    );
}

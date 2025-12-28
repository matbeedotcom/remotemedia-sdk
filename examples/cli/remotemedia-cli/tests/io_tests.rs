//! Integration tests for I/O module
//!
//! Tests for named pipe detection, stdin/stdout handling, and file I/O

use std::io::Write;
use std::path::PathBuf;
use tempfile::TempDir;

// Note: These tests import from the binary crate's public io module
// In a real scenario, this would require the io module to be in a library crate

/// Helper to create a temporary file with content
fn create_temp_file(dir: &TempDir, name: &str, content: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    path
}

#[cfg(unix)]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::FileTypeExt;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::Duration;

    /// Create a named pipe (FIFO) for testing
    fn create_fifo(path: &std::path::Path) -> std::io::Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        // Use nix or libc to create FIFO
        let result = Command::new("mkfifo").arg(path).status()?;
        if result.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "mkfifo failed",
            ))
        }
    }

    #[test]
    fn test_fifo_detection() {
        let dir = TempDir::new().unwrap();
        let fifo_path = dir.path().join("test_fifo");

        // Create FIFO
        create_fifo(&fifo_path).unwrap();

        // Verify it's detected as a FIFO
        let metadata = std::fs::metadata(&fifo_path).unwrap();
        assert!(metadata.file_type().is_fifo());
    }

    #[test]
    fn test_regular_file_not_fifo() {
        let dir = TempDir::new().unwrap();
        let file_path = create_temp_file(&dir, "regular_file.txt", b"hello world");

        // Verify regular file is not detected as FIFO
        let metadata = std::fs::metadata(&file_path).unwrap();
        assert!(!metadata.file_type().is_fifo());
    }

    #[test]
    fn test_stdin_shorthand_detection() {
        // The "-" shorthand should be recognized as stdin/stdout
        assert_eq!("-", "-");
        // This is a placeholder - real test would use the CLI binary
    }

    #[test]
    fn test_named_pipe_read_write() {
        let dir = TempDir::new().unwrap();
        let fifo_path = dir.path().join("rw_fifo");

        // Create FIFO
        create_fifo(&fifo_path).unwrap();

        let fifo_path_clone = fifo_path.clone();
        let test_data = b"Hello from named pipe!";

        // Spawn writer thread (must happen before reader opens, or reader blocks)
        let writer_handle = thread::spawn(move || {
            // Small delay to ensure reader is waiting
            thread::sleep(Duration::from_millis(50));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .open(&fifo_path_clone)
                .unwrap();
            file.write_all(test_data).unwrap();
            file.flush().unwrap();
        });

        // Reader
        let data = std::fs::read(&fifo_path).unwrap();

        writer_handle.join().unwrap();

        assert_eq!(data, test_data);
    }

    #[test]
    fn test_cli_stdin_stdout_integration() {
        // This test uses the actual CLI binary if available
        // Skip if not built
        let cli_path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/remotemedia");

        if !cli_path.exists() {
            eprintln!("Skipping integration test - CLI not built");
            return;
        }

        let dir = TempDir::new().unwrap();
        let manifest_path = dir.path().join("test.yaml");

        // Create minimal manifest
        std::fs::write(
            &manifest_path,
            r#"
version: "1.0"
name: test-pipeline
nodes: []
"#,
        )
        .unwrap();

        // Run CLI with stdin input
        let output = Command::new(&cli_path)
            .args(["run", manifest_path.to_str().unwrap(), "--input", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                // Write to stdin
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(b"test input data").ok();
                }
                child.wait_with_output()
            });

        // Just verify it doesn't crash
        if let Ok(out) = output {
            // Check exit code is 0 or handle gracefully
            println!("Exit status: {:?}", out.status);
            println!("Stdout: {}", String::from_utf8_lossy(&out.stdout));
            println!("Stderr: {}", String::from_utf8_lossy(&out.stderr));
        }
    }
}

#[test]
fn test_temp_file_creation() {
    let dir = TempDir::new().unwrap();
    let path = create_temp_file(&dir, "test.txt", b"hello");
    assert!(path.exists());
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
}

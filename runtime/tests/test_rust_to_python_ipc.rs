//! Integration test: Rust publisher → Python subscriber
//!
//! This test verifies end-to-end IPC communication by:
//! 1. Creating an iceoryx2 channel (Rust)
//! 2. Spawning a Python subprocess that subscribes
//! 3. Publishing data from Rust
//! 4. Verifying Python receives the data

#[cfg(all(test, feature = "multiprocess"))]
mod rust_to_python_ipc {
    use remotemedia_runtime::python::multiprocess::{ChannelRegistry, data_transfer::RuntimeData};
    use std::process::{Command, Stdio};
    use std::time::Duration;

    #[tokio::test]
    async fn test_rust_publishes_python_receives() {
        // Create channel registry
        let mut registry = ChannelRegistry::new();
        registry.initialize().expect("Failed to initialize");

        // Create test channel
        let channel_name = "rust_to_python_test";
        registry
            .create_channel(channel_name, 100, false)
            .await
            .expect("Failed to create channel");

        println!("📡 Created IPC channel: {}", channel_name);

        // Spawn Python subscriber process
        let python_script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("test_ipc_subscriber.py");

        println!("🐍 Spawning Python subscriber: {:?}", python_script);

        let mut python_child = Command::new("python")
            .arg(python_script)
            .arg(channel_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn Python process");

        // Give Python time to start and connect
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Create publisher and send data
        let publisher = registry
            .create_publisher(channel_name)
            .await
            .expect("Failed to create publisher");

        let test_message = "Hello from Rust test!";
        let test_data = RuntimeData::text(test_message, "test_session");

        println!("📤 Publishing: '{}'", test_message);
        publisher.publish(test_data).expect("Failed to publish");
        println!("✅ Published successfully");

        // Wait for Python to process and exit (with timeout)
        let start = tokio::time::Instant::now();
        let timeout_duration = Duration::from_secs(12);

        loop {
            // Check if process has exited
            match python_child.try_wait() {
                Ok(Some(status)) => {
                    // Process exited, collect output
                    let output = python_child.wait_with_output().unwrap_or_else(|_| {
                        panic!("Failed to get output after process exited");
                    });

                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    println!("\n--- Python stdout ---");
                    println!("{}", stdout);
                    println!("\n--- Python stderr ---");
                    println!("{}", stderr);
                    println!("---");

                    // Check exit code
                    if status.success() {
                        println!("✅ Python process exited successfully");

                        // Verify Python received the message
                        assert!(
                            stdout.contains("RECEIVED") || stdout.contains("Text message"),
                            "Python should have received the message"
                        );
                        assert!(
                            stdout.contains(test_message),
                            "Python should have received our exact message"
                        );
                    } else {
                        panic!("❌ Python process failed with exit code: {:?}", status.code());
                    }
                    break;
                }
                Ok(None) => {
                    // Still running, check timeout
                    if start.elapsed() > timeout_duration {
                        let _ = python_child.kill();
                        panic!("❌ Test timed out - Python didn't receive message in 12s");
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    panic!("❌ Error checking Python process status: {}", e);
                }
            }
        }
    }
}

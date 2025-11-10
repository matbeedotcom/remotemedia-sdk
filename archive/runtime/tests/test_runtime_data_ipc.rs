//! Test Rust → Python IPC with RuntimeData objects

#[cfg(all(test, feature = "multiprocess"))]
mod runtime_data_ipc_tests {
    use remotemedia_runtime::python::multiprocess::{
        data_transfer::RuntimeData as IPCData, ChannelRegistry,
    };
    use std::process::{Command, Stdio};
    use std::time::Duration;

    #[tokio::test]
    async fn test_text_runtime_data_to_python() {
        // Create channel registry
        let mut registry = ChannelRegistry::new();
        registry.initialize().expect("Failed to initialize");

        // Create test channel
        let channel_name = "runtime_data_text_test";
        registry
            .create_channel(channel_name, 100, false)
            .await
            .expect("Failed to create channel");

        println!("📡 Created IPC channel: {}", channel_name);

        // Spawn Python subscriber that uses RuntimeData
        let python_script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("test_ipc_subscriber_runtime_data.py");

        println!(
            "🐍 Spawning Python RuntimeData subscriber: {:?}",
            python_script
        );

        let mut python_child = Command::new("python")
            .arg(python_script)
            .arg(channel_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn Python process");

        // Give Python time to connect
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Create publisher and send RuntimeData
        let publisher = registry
            .create_publisher(channel_name)
            .await
            .expect("Failed to create publisher");

        let test_text = "Hello RuntimeData from Rust!";
        let test_data = IPCData::text(test_text, "runtime_data_session");

        println!("📤 Publishing RuntimeData::Text: '{}'", test_text);
        publisher.publish(test_data).expect("Failed to publish");
        println!("✅ Published successfully");

        // Wait for Python to process
        let start = tokio::time::Instant::now();
        let timeout_duration = Duration::from_secs(12);

        loop {
            match python_child.try_wait() {
                Ok(Some(_status)) => {
                    let output = python_child.wait_with_output().unwrap();
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);

                    println!("\n--- Python stdout ---");
                    println!("{}", stdout);
                    if !stderr.trim().is_empty() && !stderr.contains("User::from_uid") {
                        println!("\n--- Python stderr ---");
                        println!("{}", stderr);
                    }
                    println!("---");

                    assert!(output.status.success(), "Python process should succeed");
                    assert!(stdout.contains("RECEIVED"), "Should receive IPC message");
                    assert!(stdout.contains("Text RuntimeData"), "Should parse as Text");
                    assert!(stdout.contains(test_text), "Should contain our message");
                    assert!(
                        stdout.contains("is_text(): True"),
                        "RuntimeData.is_text() should work"
                    );
                    assert!(stdout.contains("SUCCESS"), "Should report success");

                    println!("✅ All assertions passed!");
                    break;
                }
                Ok(None) => {
                    if start.elapsed() > timeout_duration {
                        let _ = python_child.kill();
                        panic!("Test timed out - Python didn't receive in 12s");
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => panic!("Error waiting for Python: {}", e),
            }
        }
    }

    #[tokio::test]
    async fn test_audio_runtime_data_to_python() {
        // Create channel registry
        let mut registry = ChannelRegistry::new();
        registry.initialize().expect("Failed to initialize");

        // Create test channel
        let channel_name = "runtime_data_audio_test";
        registry
            .create_channel(channel_name, 100, false)
            .await
            .expect("Failed to create channel");

        println!("📡 Created IPC channel for audio: {}", channel_name);

        // Spawn Python subscriber
        let python_script = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("test_ipc_subscriber_runtime_data.py");

        let mut python_child = Command::new("python")
            .arg(python_script)
            .arg(channel_name)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn Python process");

        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Create and publish audio RuntimeData
        let publisher = registry
            .create_publisher(channel_name)
            .await
            .expect("Failed to create publisher");

        // Create 1000 audio samples
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001).collect();
        let audio_data = IPCData::audio(&samples, 24000, 1, "audio_test_session");

        println!("📤 Publishing RuntimeData::Audio: 1000 samples @ 24kHz");
        publisher.publish(audio_data).expect("Failed to publish");
        println!("✅ Published successfully");

        // Wait for Python
        let start = tokio::time::Instant::now();
        let timeout_duration = Duration::from_secs(12);

        loop {
            match python_child.try_wait() {
                Ok(Some(_status)) => {
                    let output = python_child.wait_with_output().unwrap();
                    let stdout = String::from_utf8_lossy(&output.stdout);

                    println!("\n--- Python stdout ---");
                    println!("{}", stdout);
                    println!("---");

                    assert!(output.status.success(), "Python process should succeed");
                    assert!(stdout.contains("RECEIVED"), "Should receive IPC message");
                    assert!(
                        stdout.contains("Audio RuntimeData"),
                        "Should parse as Audio"
                    );
                    assert!(stdout.contains("1000 samples"), "Should have 1000 samples");
                    assert!(stdout.contains("SUCCESS"), "Should report success");

                    println!("✅ Audio RuntimeData test passed!");
                    break;
                }
                Ok(None) => {
                    if start.elapsed() > timeout_duration {
                        let _ = python_child.kill();
                        panic!("Test timed out");
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => panic!("Error: {}", e),
            }
        }
    }
}

//! Integration test for Rust-to-Python IPC communication
//!
//! This test verifies that:
//! 1. Rust can create IPC channels
//! 2. Rust can publish data to the channel
//! 3. Python can receive the data
//! 4. Python can publish responses
//! 5. Rust can receive Python's responses

#[cfg(all(test, feature = "multiprocess"))]
mod ipc_tests {
    use remotemedia_runtime::data::RuntimeData;
    use remotemedia_runtime::executor::node_executor::{NodeContext, NodeExecutor};
    use remotemedia_runtime::python::multiprocess::{
        ChannelRegistry, MultiprocessConfig, MultiprocessExecutor,
    };
    use std::collections::HashMap;
    use std::time::Duration;
    use tokio::time::timeout;

    /// Test basic IPC round-trip communication
    #[tokio::test]
    async fn test_ipc_roundtrip_text() {
        // Initialize executor
        let config = MultiprocessConfig::default();
        let mut executor = MultiprocessExecutor::new(config);

        // Create test context for a simple echo node
        let ctx = NodeContext {
            node_id: "test_echo".to_string(),
            node_type: "EchoNode".to_string(),
            params: serde_json::json!({}),
            session_id: Some("test_session".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize the node (spawns Python process)
        println!("🚀 Initializing executor with EchoNode...");
        executor.initialize(&ctx)
            .await
            .expect("Failed to initialize node");
        println!("✅ Executor initialized");

        // Wait a bit for Python process to start and connect IPC
        println!("⏳ Waiting 5 seconds for Python process to start...");
        tokio::time::sleep(Duration::from_secs(5)).await;
        println!("✅ Wait complete, sending test data...");

        // Send test data
        let input_data = RuntimeData::Text("Hello from Rust!".to_string());
        let mut outputs = Vec::new();

        let result = timeout(
            Duration::from_secs(15),
            executor.process_runtime_data_streaming(
                input_data,
                Some("test_session".to_string()),
                |output| {
                    println!("✅ Received output: {:?}", output);
                    outputs.push(output);
                    Ok(())
                },
            ),
        )
        .await;

        // Check result
        match result {
            Ok(Ok(count)) => {
                println!("✅ Test passed: received {} outputs", count);
                assert!(count > 0, "Should receive at least one output");

                // Verify we got text back
                assert!(!outputs.is_empty(), "Should have outputs");
            }
            Ok(Err(e)) => {
                panic!("❌ Processing failed: {}", e);
            }
            Err(_) => {
                panic!("❌ Test timed out waiting for response");
            }
        }

        // Cleanup
        executor.cleanup()
            .await
            .expect("Failed to cleanup");
    }

    /// Test IPC communication with audio data
    #[tokio::test]
    async fn test_ipc_roundtrip_audio() {
        use remotemedia_runtime::grpc_service::AudioBuffer;

        // Initialize executor
        let config = MultiprocessConfig::default();
        let mut executor = MultiprocessExecutor::new(config);

        // Create test context
        let ctx = NodeContext {
            node_id: "test_audio_echo".to_string(),
            node_type: "EchoNode".to_string(),
            params: serde_json::json!({}),
            session_id: Some("test_audio_session".to_string()),
            metadata: HashMap::new(),
        };

        // Initialize
        executor.initialize(&ctx)
            .await
            .expect("Failed to initialize node");

        tokio::time::sleep(Duration::from_secs(3)).await;

        // Create test audio data
        let samples: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001).collect();
        let samples_bytes: Vec<u8> = samples
            .iter()
            .flat_map(|&f| f.to_le_bytes())
            .collect();

        let audio_input = RuntimeData::Audio(AudioBuffer {
            samples: samples_bytes,
            sample_rate: 24000,
            channels: 1,
            format: remotemedia_runtime::grpc_service::AudioFormat::F32 as i32,
            num_samples: 1000,
        });

        let mut outputs = Vec::new();

        let result = timeout(
            Duration::from_secs(15),
            executor.process_runtime_data_streaming(
                audio_input,
                Some("test_audio_session".to_string()),
                |output| {
                    println!("✅ Received audio output");
                    outputs.push(output);
                    Ok(())
                },
            ),
        )
        .await;

        match result {
            Ok(Ok(count)) => {
                println!("✅ Audio test passed: received {} outputs", count);
                assert!(count > 0, "Should receive at least one output");
            }
            Ok(Err(e)) => {
                panic!("❌ Audio processing failed: {}", e);
            }
            Err(_) => {
                panic!("❌ Audio test timed out");
            }
        }

        // Cleanup
        executor.cleanup()
            .await
            .expect("Failed to cleanup");
    }

    /// Test that IPC channels are properly created with correct names
    #[tokio::test]
    async fn test_ipc_channel_creation() {
        // Create and initialize channel registry directly
        let mut registry = ChannelRegistry::new();
        registry.initialize().expect("Failed to initialize channel registry");

        // Create test channels
        let channel = registry
            .create_channel("test_ipc_channel", 10, false)
            .await
            .expect("Failed to create channel");

        assert_eq!(channel.name, "test_ipc_channel");
        assert_eq!(channel.capacity, 10);

        // Verify we can create publisher and subscriber
        let publisher = registry
            .create_publisher("test_ipc_channel")
            .await
            .expect("Failed to create publisher");

        let subscriber = registry
            .create_subscriber("test_ipc_channel")
            .await
            .expect("Failed to create subscriber");

        // Test publish/receive
        use remotemedia_runtime::python::multiprocess::data_transfer::RuntimeData as IPCData;
        let test_data = IPCData::text("Test message", "test_session");

        println!("📤 Publishing test message...");
        publisher
            .publish(test_data)
            .expect("Failed to publish");

        // Give it a moment
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("📥 Attempting to receive...");
        let received = subscriber
            .receive()
            .expect("Failed to receive");

        assert!(received.is_some(), "Should receive published data");
        if let Some(data) = received {
            println!("✅ Received data for session: {}", data.session_id);
            assert_eq!(data.session_id, "test_session");
        }
    }
}


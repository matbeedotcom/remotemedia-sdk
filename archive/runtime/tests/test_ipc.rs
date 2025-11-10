//! IPC channel tests for zero-copy data transfer

#[cfg(test)]
#[cfg(feature = "multiprocess")]
mod tests {
    use remotemedia_runtime::python::multiprocess::{
        data_transfer::{DataType, RuntimeData},
        ipc_channel::ChannelRegistry,
    };
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn test_data_serialization_roundtrip() {
        // Test text data
        let text_data = RuntimeData::text("Hello, IPC!", "test_session");
        let bytes = text_data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type as u8, DataType::Text as u8);
        assert_eq!(recovered.session_id, "test_session");
        assert_eq!(String::from_utf8_lossy(&recovered.payload), "Hello, IPC!");

        // Test audio data
        let audio_samples = vec![0.1f32, 0.2, 0.3, 0.4, 0.5];
        let audio_data = RuntimeData::audio(&audio_samples, 24000, 1, "audio_session");
        let bytes = audio_data.to_bytes();
        let recovered = RuntimeData::from_bytes(&bytes).unwrap();

        assert_eq!(recovered.data_type as u8, DataType::Audio as u8);
        assert_eq!(recovered.session_id, "audio_session");
        assert_eq!(recovered.payload.len(), 20); // 5 f32s = 20 bytes
    }

    #[tokio::test]
    async fn test_channel_lifecycle() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        // Create channel
        let channel = registry
            .create_channel("test_channel", 100, true)
            .await
            .unwrap();
        assert_eq!(channel.name, "test_channel");
        assert_eq!(channel.capacity, 100);
        assert!(channel.backpressure_enabled);

        // Verify stats are initialized
        let stats = channel.stats.read().await;
        assert_eq!(stats.messages_sent, 0);
        assert_eq!(stats.messages_received, 0);
        assert_eq!(stats.bytes_transferred, 0);
        drop(stats);

        // Cleanup
        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    async fn test_publisher_subscriber() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry
            .create_channel("pubsub_test", 10, false)
            .await
            .unwrap();

        // Create publisher and subscriber
        let publisher = registry.create_publisher("pubsub_test").await.unwrap();
        let subscriber = registry.create_subscriber("pubsub_test").await.unwrap();

        // Publish data with very small payload to stay within iceoryx2 defaults
        let data = RuntimeData::text("Hi", "s"); // Minimal payload
        publisher.publish(data).await.unwrap();

        // Small delay to allow message to propagate
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Receive data
        let received = subscriber.receive().await.unwrap();
        assert!(received.is_some(), "Should receive a message");

        let received_data = received.unwrap();
        assert_eq!(received_data.session_id, "s");
        assert_eq!(String::from_utf8_lossy(&received_data.payload), "Hi");

        // Cleanup
        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry
            .create_channel("multi_msg", 50, false)
            .await
            .unwrap();

        let publisher = registry.create_publisher("multi_msg").await.unwrap();
        let subscriber = registry.create_subscriber("multi_msg").await.unwrap();

        // Send multiple small messages
        for i in 0..10 {
            let data = RuntimeData::text(&format!("{}", i), "s"); // Minimal payload
            publisher.publish(data).await.unwrap();
        }

        // Small delay
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Receive and verify messages
        let mut received_count = 0;
        while let Some(_data) = subscriber.receive().await.unwrap() {
            received_count += 1;

            if received_count >= 10 {
                break;
            }
        }

        assert_eq!(received_count, 10, "Should receive all 10 messages");

        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    async fn test_moderate_payload() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry
            .create_channel("moderate_payload", 10, false)
            .await
            .unwrap();

        let publisher = registry.create_publisher("moderate_payload").await.unwrap();
        let subscriber = registry
            .create_subscriber("moderate_payload")
            .await
            .unwrap();

        // Create 1KB payload (well within defaults)
        let moderate_data = vec![0u8; 1024];
        let data = RuntimeData {
            data_type: DataType::Audio,
            session_id: "s".to_string(),
            timestamp: 12345,
            payload: moderate_data,
        };

        // Measure publish time
        let start = Instant::now();
        publisher.publish(data).await.unwrap();
        let publish_time = start.elapsed();

        println!("Published 1KB in {:?}", publish_time);

        // Small delay
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Measure receive time
        let start = Instant::now();
        let received = subscriber.receive().await.unwrap();
        let receive_time = start.elapsed();

        println!("Received 1KB in {:?}", receive_time);

        assert!(received.is_some());
        let received_data = received.unwrap();
        assert_eq!(received_data.payload.len(), 1024);

        // Total latency should be very low
        let total_latency = publish_time + receive_time;
        println!("Total latency for 1KB: {:?}", total_latency);

        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    async fn test_channel_stats() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry
            .create_channel("stats_test", 10, false)
            .await
            .unwrap();

        let publisher = registry.create_publisher("stats_test").await.unwrap();
        let subscriber = registry.create_subscriber("stats_test").await.unwrap();

        // Send data
        let data = RuntimeData::text("x", "s"); // Minimal
        let bytes_len = data.to_bytes().len();

        publisher.publish(data).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Verify publisher stats
        {
            let stats = channel.stats.read().await;
            assert_eq!(stats.messages_sent, 1);
            assert!(stats.bytes_transferred >= bytes_len as u64);
            assert!(stats.last_activity.is_some());
        }

        // Receive data
        let _received = subscriber.receive().await.unwrap();

        // Verify subscriber stats
        {
            let stats = channel.stats.read().await;
            assert_eq!(stats.messages_received, 1);
        }

        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    async fn test_no_message_available() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry
            .create_channel("empty_test", 10, false)
            .await
            .unwrap();

        let subscriber = registry.create_subscriber("empty_test").await.unwrap();

        // Try to receive from empty channel
        let received = subscriber.receive().await.unwrap();
        assert!(
            received.is_none(),
            "Should return None when no messages available"
        );

        registry.destroy_channel(channel).await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_publishers() {
        let mut registry = ChannelRegistry::new();
        registry.initialize().unwrap();

        let channel = registry
            .create_channel("concurrent", 100, false)
            .await
            .unwrap();

        // Create multiple publishers
        let pub1 = registry.create_publisher("concurrent").await.unwrap();
        let pub2 = registry.create_publisher("concurrent").await.unwrap();
        let subscriber = registry.create_subscriber("concurrent").await.unwrap();

        // Publish from both using join to run concurrently
        let publish1 = async {
            for i in 0..5 {
                let data = RuntimeData::text(&format!("{}", i), "s"); // Minimal
                pub1.publish(data).await.unwrap();
            }
        };

        let publish2 = async {
            for i in 0..5 {
                let data = RuntimeData::text(&format!("{}", i), "s"); // Minimal
                pub2.publish(data).await.unwrap();
            }
        };

        // Run both publishers concurrently
        tokio::join!(publish1, publish2);

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Receive all messages
        let mut received_count = 0;
        while subscriber.receive().await.unwrap().is_some() {
            received_count += 1;
            if received_count >= 10 {
                break;
            }
        }

        assert_eq!(
            received_count, 10,
            "Should receive all messages from both publishers"
        );

        registry.destroy_channel(channel).await.unwrap();
    }
}

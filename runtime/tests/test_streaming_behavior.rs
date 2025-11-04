//! Unit test to verify that our async streaming architecture
//! processes chunks immediately as they arrive

use remotemedia_runtime::data::RuntimeData;
use remotemedia_runtime::grpc_service::session_router::{SessionRouter, DataPacket};
use remotemedia_runtime::grpc_service::streaming::StreamSession;
use remotemedia_runtime::grpc_service::generated::{StreamResponse, stream_response};
use remotemedia_runtime::manifest::{Manifest, NodeSpec, Connection};
use remotemedia_runtime::nodes::StreamingNodeRegistry;
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tonic::Status;
use tracing::info;

/// Track when chunks arrive at the "client"
#[derive(Debug, Clone)]
struct ChunkTiming {
    timestamp: Instant,
    sequence: u64,
    from_node: String,
}

/// Test that verifies chunks are processed and routed immediately
#[tokio::test]
async fn test_chunks_are_routed_immediately() {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("remotemedia_runtime=info")
        .try_init();

    info!("Testing immediate chunk routing...");

    // Track chunk timings
    let chunk_timings = Arc::new(Mutex::new(Vec::<ChunkTiming>::new()));
    let test_start = Instant::now();

    // Create a simple manifest
    let manifest = Manifest {
        version: "1.0".to_string(),
        nodes: vec![
            NodeSpec {
                id: "passthrough1".to_string(),
                node_type: "Passthrough".to_string(),
                params: json!({}),
            },
            NodeSpec {
                id: "passthrough2".to_string(),
                node_type: "Passthrough".to_string(),
                params: json!({}),
            },
        ],
        connections: vec![
            Connection {
                from: "client".to_string(),
                to: "passthrough1".to_string(),
            },
            Connection {
                from: "passthrough1".to_string(),
                to: "passthrough2".to_string(),
            },
        ],
        inputs: vec!["passthrough1".to_string()],
        outputs: vec!["passthrough2".to_string()],
    };

    // Create session state
    let session = Arc::new(Mutex::new(StreamSession::new(
        "test-session".to_string(),
        manifest.clone(),
    )));

    // Create registry
    let registry = Arc::new(StreamingNodeRegistry::new());

    // Create channel to track client outputs
    let (client_tx, mut client_rx) = mpsc::channel(32);

    // Create router
    let mut router = SessionRouter::new(
        "test-session".to_string(),
        registry,
        session.clone(),
        client_tx,
    );

    // Get input sender
    let input_sender = router.get_input_sender();

    // Start router
    let router_task = router.start();

    // Clone for tracking task
    let chunk_timings_clone = chunk_timings.clone();

    // Start task to track outputs
    let tracking_task = tokio::spawn(async move {
        while let Some(result) = client_rx.recv().await {
            if let Ok(response) = result {
                if let Some(stream_response::Response::Result(chunk)) = response.response {
                    let arrival = ChunkTiming {
                        timestamp: Instant::now(),
                        sequence: chunk.sequence,
                        from_node: chunk.data_outputs.keys().next().unwrap_or(&"unknown".to_string()).clone(),
                    };

                    let elapsed = arrival.timestamp.duration_since(test_start);
                    info!(
                        "Chunk arrived at client - seq: {}, from: {}, time: {:.3}s",
                        arrival.sequence,
                        arrival.from_node,
                        elapsed.as_secs_f64()
                    );

                    let mut timings = chunk_timings_clone.lock().await;
                    timings.push(arrival);
                }
            }
        }
    });

    // Send chunks with delays to simulate streaming
    info!("Sending chunks with delays...");
    for i in 0..5 {
        let packet = DataPacket {
            data: RuntimeData::Text(format!("Test chunk {}", i)),
            from_node: "client".to_string(),
            to_node: Some("passthrough1".to_string()),
            session_id: "test-session".to_string(),
            sequence: i,
            sub_sequence: 0,
        };

        let send_time = Instant::now().duration_since(test_start);
        info!("Sending chunk {} at {:.3}s", i, send_time.as_secs_f64());

        input_sender.send(packet).unwrap();

        // Wait 100ms between chunks
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Wait for processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Shutdown router
    drop(input_sender);
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Analyze results
    let timings = chunk_timings.lock().await;

    info!("=== Routing Timing Analysis ===");
    info!("Chunks sent: 5");
    info!("Chunks received at client: {}", timings.len());

    // Verify we received chunks
    assert!(
        timings.len() >= 3,
        "Expected at least 3 chunks to arrive, got {}",
        timings.len()
    );

    // Calculate delays
    if timings.len() >= 2 {
        for i in 1..timings.len() {
            let gap = timings[i].timestamp.duration_since(timings[i - 1].timestamp);
            info!(
                "Gap between chunk {} and {}: {:.3}s",
                i - 1,
                i,
                gap.as_secs_f64()
            );

            // Verify chunks arrive with reasonable spacing
            // Should be around 100ms since that's our send interval
            assert!(
                gap.as_millis() >= 50,
                "Chunks arrived too quickly - gap was only {}ms",
                gap.as_millis()
            );
        }
    }

    // Calculate processing latency (time from send to receive)
    // Since we sent chunks at 0ms, 100ms, 200ms, etc.
    // We can estimate the latency
    for (i, timing) in timings.iter().enumerate() {
        let expected_send_time = i as f64 * 0.1; // 100ms intervals
        let actual_arrival = timing.timestamp.duration_since(test_start).as_secs_f64();
        let latency = actual_arrival - expected_send_time;

        info!(
            "Chunk {} - sent at ~{:.3}s, arrived at {:.3}s, latency: {:.3}s",
            i, expected_send_time, actual_arrival, latency
        );

        // Verify low latency (should be < 50ms for passthrough nodes)
        assert!(
            latency < 0.1,
            "Chunk {} had high latency: {:.3}s",
            i,
            latency
        );
    }

    info!("✅ Test passed - chunks are routed immediately with low latency!");
}
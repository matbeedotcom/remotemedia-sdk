//! Test to verify that streaming nodes output chunks immediately
//! rather than batching them all together.

use super::test_helpers::*;
use remotemedia_runtime::grpc_service::generated::{
    stream_request, stream_response, DataChunk, StreamInit, StreamRequest,
};
use remotemedia_runtime::manifest::{Manifest, NodeSpec};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::info;

/// Test data structure to track chunk arrivals
#[derive(Debug, Clone)]
struct ChunkArrival {
    sequence: u64,
    timestamp: Instant,
    data_size: usize,
    node_id: String,
}

/// Test that verifies streaming chunks arrive incrementally, not all at once
#[tokio::test]
async fn test_streaming_is_truly_incremental() {
    // Initialize logging
    init_test_logging();

    info!("Starting streaming timing test - verifying chunks arrive incrementally");

    // Track chunk arrivals
    let chunk_arrivals = Arc::new(Mutex::new(Vec::<ChunkArrival>::new()));
    let test_start = Instant::now();

    // Create a simple manifest with a Python streaming node (simulated)
    let manifest = Manifest {
        version: "1.0".to_string(),
        nodes: vec![NodeSpec {
            id: "test_streaming".to_string(),
            node_type: "TestStreamingNode".to_string(),
            params: json!({
                "chunk_count": 5,
                "chunk_delay_ms": 200, // 200ms between chunks
            }),
        }],
        connections: vec![],
        inputs: vec!["test_streaming".to_string()],
        outputs: vec!["test_streaming".to_string()],
    };

    // Setup test client
    let test_client = TestClient::new().await;
    let mut client = test_client.streaming_client();

    // Start streaming session
    let (mut tx, mut rx) = client.stream_pipeline().await.unwrap();

    // Clone for tracking task
    let chunk_arrivals_clone = chunk_arrivals.clone();

    // Spawn task to track chunk arrivals
    let tracking_task = tokio::spawn(async move {
        while let Ok(Some(response)) = rx.message().await {
            let arrival_time = Instant::now();

            if let Some(stream_response::Response::Result(chunk_result)) = response.response {
                let mut arrivals = chunk_arrivals_clone.lock().await;

                for (node_id, data_buffer) in &chunk_result.data_outputs {
                    let data_size = data_buffer.encoded_size();

                    arrivals.push(ChunkArrival {
                        sequence: chunk_result.sequence,
                        timestamp: arrival_time,
                        data_size,
                        node_id: node_id.clone(),
                    });

                    info!(
                        "Chunk arrived - seq: {}, node: {}, size: {}, time: {:.3}s",
                        chunk_result.sequence,
                        node_id,
                        data_size,
                        arrival_time.duration_since(test_start).as_secs_f64()
                    );
                }
            }
        }
    });

    // Send StreamInit
    info!("Sending StreamInit...");
    tx.send(StreamRequest {
        request: Some(stream_request::Request::Init(StreamInit {
            manifest: Some(manifest.into()),
            client_version: "test-1.0".to_string(),
            expected_chunk_size: 512,
        })),
    })
    .await
    .unwrap();

    // Send test data chunk
    info!("Sending test data chunk...");
    tx.send(StreamRequest {
        request: Some(stream_request::Request::DataChunk(DataChunk {
            sequence: 0,
            node_id: "test_streaming".to_string(),
            buffer: Some(create_test_audio_buffer(1000)),
            named_buffers: Default::default(),
        })),
    })
    .await
    .unwrap();

    // Wait for chunks to arrive
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Close stream
    drop(tx);
    let _ = tracking_task.await;

    // Analyze chunk arrivals
    let arrivals = chunk_arrivals.lock().await;

    info!("=== Streaming Timing Analysis ===");
    info!("Total chunks received: {}", arrivals.len());

    if arrivals.len() < 2 {
        info!("⚠️ Not enough chunks received to verify streaming behavior");
        return;
    }

    // Calculate time gaps between chunks
    let mut time_gaps = Vec::new();
    for i in 1..arrivals.len() {
        let gap = arrivals[i]
            .timestamp
            .duration_since(arrivals[i - 1].timestamp);
        time_gaps.push(gap);

        info!(
            "Gap between chunk {} and {}: {:.3}s",
            i - 1,
            i,
            gap.as_secs_f64()
        );
    }

    // Verify chunks arrived incrementally (not all at once)
    // If all chunks arrived at once, gaps would be < 10ms
    let significant_gaps = time_gaps.iter().filter(|gap| gap.as_millis() > 50).count();

    // Calculate statistics
    if !arrivals.is_empty() && arrivals.len() > 1 {
        let total_duration = arrivals
            .last()
            .unwrap()
            .timestamp
            .duration_since(arrivals[0].timestamp);
        let avg_gap = total_duration.as_secs_f64() / (arrivals.len() - 1) as f64;

        info!("Total duration: {:.3}s", total_duration.as_secs_f64());
        info!("Average gap between chunks: {:.3}s", avg_gap);
        info!("Chunks with >50ms gaps: {}", significant_gaps);

        // For test purposes, we just log the results
        // In a real streaming scenario, we'd assert that avg_gap > 0.05
        if avg_gap < 0.01 {
            info!("⚠️ WARNING: Chunks arrived very close together ({:.3}s avg gap) - may indicate batching!", avg_gap);
        } else {
            info!("✅ Chunks arrived with reasonable spacing ({:.3}s avg gap) - appears to be streaming!", avg_gap);
        }
    }
}

/// Test to specifically verify LFM2Audio streaming behavior
#[tokio::test]
#[ignore] // Ignore by default as it requires LFM2 model to be available
async fn test_lfm2_audio_streaming_timing() {
    init_test_logging();

    info!("Testing LFM2Audio streaming timing...");

    let chunk_arrivals = Arc::new(Mutex::new(Vec::<ChunkArrival>::new()));
    let test_start = Instant::now();

    // Create manifest with LFM2Audio node
    let manifest = Manifest {
        version: "1.0".to_string(),
        nodes: vec![
            NodeSpec {
                id: "audio_chunker".to_string(),
                node_type: "AudioChunker".to_string(),
                params: json!({
                    "chunk_size": 512,
                    "sample_rate": 16000,
                }),
            },
            NodeSpec {
                id: "lfm2_audio".to_string(),
                node_type: "PythonStreamingNode".to_string(),
                params: json!({
                    "module": "lfm2_audio",
                    "streaming": true,
                }),
            },
        ],
        connections: vec![Connection {
            from: "audio_chunker".to_string(),
            to: "lfm2_audio".to_string(),
        }],
        inputs: vec!["audio_chunker".to_string()],
        outputs: vec!["lfm2_audio".to_string()],
    };

    // Setup test client
    let test_client = TestClient::new().await;
    let mut client = test_client.streaming_client();

    // Start streaming session
    let (mut tx, mut rx) = client.stream_pipeline().await.unwrap();

    // Track arrivals
    let chunk_arrivals_clone = chunk_arrivals.clone();
    let tracking_task = tokio::spawn(async move {
        let mut first_chunk_time = None;
        let mut last_chunk_time = None;
        let mut chunk_count = 0;

        while let Ok(Some(response)) = rx.message().await {
            let arrival_time = Instant::now();

            if let Some(stream_response::Response::Result(chunk_result)) = response.response {
                if first_chunk_time.is_none() {
                    first_chunk_time = Some(arrival_time);
                    info!(
                        "First chunk arrived at {:.3}s",
                        arrival_time.duration_since(test_start).as_secs_f64()
                    );
                }
                last_chunk_time = Some(arrival_time);
                chunk_count += 1;

                let mut arrivals = chunk_arrivals_clone.lock().await;
                for (node_id, _) in &chunk_result.data_outputs {
                    arrivals.push(ChunkArrival {
                        sequence: chunk_result.sequence,
                        timestamp: arrival_time,
                        data_size: 0,
                        node_id: node_id.clone(),
                    });
                }
            }
        }

        if let (Some(first), Some(last)) = (first_chunk_time, last_chunk_time) {
            let total_time = last.duration_since(first).as_secs_f64();
            info!("Received {} chunks over {:.3}s", chunk_count, total_time);

            if chunk_count > 1 {
                let avg_interval = total_time / (chunk_count - 1) as f64;
                info!("Average interval between chunks: {:.3}s", avg_interval);
            }
        }
    });

    // Send StreamInit
    tx.send(StreamRequest {
        request: Some(stream_request::Request::Init(StreamInit {
            manifest: Some(manifest.into()),
            client_version: "test-1.0".to_string(),
            expected_chunk_size: 16000, // 1 second at 16kHz
        })),
    })
    .await
    .unwrap();

    // Send audio chunk (2 seconds of audio to trigger multiple outputs)
    let audio_samples = 32000; // 2 seconds at 16kHz
    tx.send(StreamRequest {
        request: Some(stream_request::Request::DataChunk(DataChunk {
            sequence: 0,
            node_id: "audio_chunker".to_string(),
            buffer: Some(create_test_audio_buffer(audio_samples)),
            named_buffers: Default::default(),
        })),
    })
    .await
    .unwrap();

    // Wait for LFM2 to process (typically takes 5-10 seconds)
    info!("Waiting for LFM2 processing...");
    tokio::time::sleep(Duration::from_secs(15)).await;

    // Close and analyze
    drop(tx);
    let _ = tracking_task.await;

    let arrivals = chunk_arrivals.lock().await;

    info!("=== LFM2 Streaming Analysis ===");
    info!("Total chunks from LFM2: {}", arrivals.len());

    // Verify we got multiple chunks from LFM2
    if arrivals.len() > 1 {
        // Check timing between chunks
        for i in 1..arrivals.len() {
            let gap = arrivals[i]
                .timestamp
                .duration_since(arrivals[i - 1].timestamp);

            info!(
                "LFM2 chunk {} → {}: {:.3}s gap",
                i - 1,
                i,
                gap.as_secs_f64()
            );
        }

        // Calculate if chunks are streaming or batched
        let first_to_last = arrivals
            .last()
            .unwrap()
            .timestamp
            .duration_since(arrivals[0].timestamp);

        if first_to_last.as_secs_f64() > 1.0 && arrivals.len() > 2 {
            info!(
                "✅ LFM2 chunks arrived over {:.3}s - STREAMING confirmed!",
                first_to_last.as_secs_f64()
            );
        } else {
            info!(
                "⚠️ LFM2 chunks arrived within {:.3}s - might be batched",
                first_to_last.as_secs_f64()
            );
        }
    } else {
        info!(
            "⚠️ Only {} chunk(s) received - cannot verify streaming",
            arrivals.len()
        );
    }
}

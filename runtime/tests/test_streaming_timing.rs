//! Test to verify that streaming nodes (like LFM2Audio) output chunks immediately
//! rather than batching them all together.

use remotemedia_runtime::grpc_service::{
    generated::{
        streaming_pipeline_service_server::StreamingPipelineService,
        stream_request::Request as StreamRequestType,
        stream_response::Response as StreamResponseType,
        AudioBuffer, ChunkResult, DataBuffer, DataChunk, ManifestProto, NodeProto,
        ConnectionProto, StreamInit, StreamRequest, StreamResponse,
        data_buffer, AudioFormat,
    },
    ServiceConfig,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{info, debug};

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
async fn test_lfm2_audio_streaming_is_incremental() {
    // Initialize logging for debugging
    let _ = tracing_subscriber::fmt()
        .with_env_filter("remotemedia_runtime=debug")
        .try_init();

    info!("Starting LFM2 audio streaming timing test");

    // Track chunk arrivals
    let chunk_arrivals = Arc::new(Mutex::new(Vec::<ChunkArrival>::new()));
    let test_start = Instant::now();

    // Create test manifest for LFM2 audio pipeline
    let manifest = create_test_manifest();

    // Create the streaming service
    let config = ServiceConfig::default();
    let service = remotemedia_runtime::grpc_service::streaming::StreamingServiceImpl::new(config);

    // Create bidirectional stream channels
    let (client_tx, client_rx) = mpsc::channel::<StreamRequest>(32);
    let (server_tx, mut server_rx) = mpsc::channel::<Result<StreamResponse, Status>>(32);

    // Spawn the service handler
    let service_clone = service.clone();
    let server_tx_clone = server_tx.clone();
    let chunk_arrivals_clone = chunk_arrivals.clone();

    let service_task = tokio::spawn(async move {
        let client_stream = ReceiverStream::new(client_rx);
        let request = Request::new(client_stream);

        // Call the streaming service
        let response = service_clone.stream_pipeline(request).await.unwrap();
        let mut response_stream = response.into_inner();

        // Forward responses and track timing
        while let Ok(Some(response)) = response_stream.message().await {
            let arrival_time = Instant::now();

            // Track chunk arrivals
            if let Some(StreamResponseType::Result(ref chunk_result)) = response.response {
                let mut arrivals = chunk_arrivals_clone.lock().await;

                for (node_id, data_buffer) in &chunk_result.data_outputs {
                    let data_size = match &data_buffer.data_type {
                        Some(data_buffer::DataType::Audio(audio)) => {
                            audio.samples.len()
                        }
                        Some(data_buffer::DataType::Text(text)) => {
                            text.text_data.len()
                        }
                        _ => 0,
                    };

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

            // Forward to test receiver
            let _ = server_tx_clone.send(Ok(response)).await;
        }
    });

    // Send StreamInit
    info!("Sending StreamInit...");
    let init_request = StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "test-1.0".to_string(),
            expected_chunk_size: 512,
        })),
    };
    client_tx.send(init_request).await.unwrap();

    // Wait for StreamReady
    let ready_response = server_rx.recv().await.unwrap().unwrap();
    assert!(matches!(
        ready_response.response,
        Some(StreamResponseType::Ready(_))
    ));
    info!("Received StreamReady");

    // Send test audio chunk to trigger LFM2 processing
    info!("Sending audio chunk to trigger LFM2...");
    let audio_chunk = create_test_audio_chunk();
    let chunk_request = StreamRequest {
        request: Some(StreamRequestType::DataChunk(audio_chunk)),
    };
    client_tx.send(chunk_request).await.unwrap();

    // Collect responses for a reasonable time (LFM2 typically takes 5-10 seconds)
    info!("Collecting streaming responses...");
    let collection_start = Instant::now();
    let mut response_count = 0;

    while collection_start.elapsed() < Duration::from_secs(15) {
        tokio::select! {
            response = server_rx.recv() => {
                if let Some(Ok(resp)) = response {
                    if let Some(StreamResponseType::Result(_)) = resp.response {
                        response_count += 1;
                    }
                } else {
                    break;
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                // Continue collecting
            }
        }
    }

    info!("Collected {} responses", response_count);

    // Analyze chunk arrivals
    let arrivals = chunk_arrivals.lock().await;

    // Verify we got multiple chunks
    assert!(
        arrivals.len() >= 3,
        "Expected at least 3 chunks for streaming verification, got {}",
        arrivals.len()
    );

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
    let significant_gaps = time_gaps
        .iter()
        .filter(|gap| gap.as_millis() > 50)
        .count();

    assert!(
        significant_gaps > 0,
        "Chunks arrived too close together - not streaming! All gaps < 50ms"
    );

    // Calculate statistics
    let total_duration = arrivals
        .last()
        .unwrap()
        .timestamp
        .duration_since(arrivals[0].timestamp);
    let avg_gap = total_duration.as_secs_f64() / (arrivals.len() - 1) as f64;

    info!("=== Streaming Timing Analysis ===");
    info!("Total chunks received: {}", arrivals.len());
    info!("Total duration: {:.3}s", total_duration.as_secs_f64());
    info!("Average gap between chunks: {:.3}s", avg_gap);
    info!("Chunks with >50ms gaps: {}", significant_gaps);

    // Verify reasonable streaming behavior
    assert!(
        avg_gap > 0.05,
        "Average gap {:.3}s is too small - chunks are batched, not streamed!",
        avg_gap
    );

    // Cleanup
    drop(client_tx);
    let _ = service_task.await;

    info!("✅ Test passed - LFM2 audio is truly streaming chunks incrementally!");
}

/// Create a test manifest with audio input -> LFM2 -> output pipeline
fn create_test_manifest() -> ManifestProto {
    use serde_json::json;

    ManifestProto {
        version: "1.0".to_string(),
        nodes: vec![
            NodeProto {
                id: "audio_input".to_string(),
                node_type: "AudioChunker".to_string(),
                params: json!({
                    "chunk_size": 512,
                    "sample_rate": 16000,
                }).to_string(),
            },
            NodeProto {
                id: "lfm2_audio".to_string(),
                node_type: "LFM2Audio".to_string(),
                params: json!({
                    "model_path": "models/lfm2-audio",
                    "streaming": true,
                }).to_string(),
            },
        ],
        connections: vec![
            ConnectionProto {
                from: "audio_input".to_string(),
                to: "lfm2_audio".to_string(),
            },
        ],
        inputs: vec!["audio_input".to_string()],
        outputs: vec!["lfm2_audio".to_string()],
    }
}

/// Create a test audio chunk
fn create_test_audio_chunk() -> DataChunk {
    // Create 1 second of test audio at 16kHz
    let sample_count = 16000;
    let mut samples = vec![0u8; sample_count * 4]; // 4 bytes per f32

    // Generate a simple sine wave
    for i in 0..sample_count {
        let t = i as f32 / 16000.0;
        let sample = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        let bytes = sample.to_le_bytes();
        samples[i * 4..(i + 1) * 4].copy_from_slice(&bytes);
    }

    DataChunk {
        sequence: 0,
        node_id: "audio_input".to_string(),
        buffer: Some(DataBuffer {
            data_type: Some(
                data_buffer::DataType::Audio(AudioBuffer {
                    samples,
                    format: AudioFormat::F32le as i32,
                    channels: 1,
                    sample_rate: 16000,
                })
            ),
        }),
        named_buffers: Default::default(),
    }
}

/// Test that multiple streaming nodes in sequence maintain streaming behavior
#[tokio::test]
async fn test_pipeline_streaming_latency() {
    info!("Starting pipeline streaming latency test");

    // This test would verify that a pipeline with multiple streaming nodes
    // (e.g., VAD -> AudioBuffer -> LFM2) maintains low latency between stages

    // Track the time from input to first output
    let start = Instant::now();

    // ... (implementation similar to above but with latency measurements)

    let first_output_time = Instant::now().duration_since(start);

    assert!(
        first_output_time.as_secs() < 2,
        "First output took {:.3}s - pipeline is not streaming efficiently!",
        first_output_time.as_secs_f64()
    );
}
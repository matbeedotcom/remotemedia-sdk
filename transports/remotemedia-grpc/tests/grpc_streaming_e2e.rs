//! End-to-end integration test for gRPC streaming service
//!
//! This test validates the complete flow:
//! 1. Start gRPC server
//! 2. Connect client
//! 3. Create streaming session with manifest
//! 4. Send video frames through pipeline
//! 5. Receive processed video frames
//! 6. Verify VideoFlip node processes frames correctly

use remotemedia_grpc::generated::{
    stream_request::Request as StreamRequestType, stream_response::Response as StreamResponseType,
    streaming_pipeline_service_client::StreamingPipelineServiceClient, AudioBuffer, AudioChunk,
    StreamControl, StreamInit, StreamRequest, StreamResponse,
};
use remotemedia_grpc::{ServiceConfig, StreamingServiceImpl};
use remotemedia_runtime_core::transport::PipelineRunner;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};
use tonic::transport::Server;
use tonic::{Request, Streaming};

/// Start gRPC server in background
async fn start_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let addr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    let server_url = format!("http://{}", local_addr);

    // Create PipelineRunner
    let runner = Arc::new(PipelineRunner::new().unwrap());

    // Create service
    let config = ServiceConfig::default();
    let service = StreamingServiceImpl::new(
        config.auth,
        config.limits,
        config.metrics.clone(),
        runner,
    );

    // Spawn server
    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(
                remotemedia_grpc::generated::streaming_pipeline_service_server::StreamingPipelineServiceServer::new(
                    service,
                ),
            )
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server time to start
    sleep(Duration::from_millis(100)).await;

    (server_url, handle)
}

/// Create a test manifest with VideoFlip node
fn create_test_manifest() -> remotemedia_grpc::generated::Manifest {
    use remotemedia_grpc::generated::{manifest::Node, manifest::Params, Manifest};

    let manifest_json = r#"{
        "direction": "vertical"
    }"#;

    Manifest {
        version: "v1".to_string(),
        name: "video-flip-test".to_string(),
        description: Some("Test pipeline with VideoFlip node".to_string()),
        nodes: vec![Node {
            id: "flip".to_string(),
            node_type: "VideoFlip".to_string(),
            params: Some(Params {
                json: manifest_json.to_string(),
            }),
        }],
        connections: vec![],
    }
}

/// Create test video frame (2x2 RGB24)
fn create_test_video_frame(frame_number: u64) -> AudioBuffer {
    // Create 2x2 RGB24 image
    // Top row: red, green
    // Bottom row: blue, white
    let pixels = vec![
        255, 0, 0, // red
        0, 255, 0, // green
        0, 0, 255, // blue
        255, 255, 255, // white
    ];

    AudioBuffer {
        data: pixels,
        sample_rate: 0,           // Not used for video
        channels: 0,              // Not used for video
        num_samples: frame_number, // Abuse this field to pass frame number
        format: 1,                // RGB24
        timestamp_us: frame_number * 33333, // ~30 FPS
    }
}

#[tokio::test]
async fn test_grpc_streaming_video_pipeline() {
    println!("\n🧪 gRPC Streaming E2E Test Starting...\n");

    // Step 1: Start gRPC server
    println!("📡 Step 1: Starting gRPC server...");
    let (server_url, _server_handle) = start_test_server().await;
    println!("  ✓ Server started at {}", server_url);

    // Step 2: Connect client
    println!("\n🔌 Step 2: Connecting client to server...");
    let mut client = timeout(
        Duration::from_secs(5),
        StreamingPipelineServiceClient::connect(server_url.clone()),
    )
    .await
    .expect("Timeout connecting to server")
    .expect("Failed to connect client");
    println!("  ✓ Client connected");

    // Step 3: Create streaming session
    println!("\n📝 Step 3: Creating streaming session with manifest...");
    let (mut request_tx, response_rx) = tokio::sync::mpsc::channel(32);

    // Send StreamInit
    let manifest = create_test_manifest();
    let init_request = StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            client_version: "test-v1.0".to_string(),
            manifest: Some(manifest),
            preview_features: vec![],
        })),
    };

    request_tx.send(init_request).await.unwrap();

    // Create streaming request
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(response_rx);
    let request = Request::new(request_stream);

    // Start bidirectional stream
    let mut response_stream = client
        .stream_pipeline(request)
        .await
        .expect("Failed to start stream")
        .into_inner();

    // Wait for StreamReady
    let ready_response = timeout(Duration::from_secs(5), response_stream.message())
        .await
        .expect("Timeout waiting for StreamReady")
        .expect("Stream error")
        .expect("No response");

    match ready_response.response {
        Some(StreamResponseType::Ready(ready)) => {
            println!("  ✓ Session created: {}", ready.session_id);
            println!("  ✓ Server version: {}", ready.server_version);
        }
        _ => panic!("Expected StreamReady response"),
    }

    // Step 4: Send video frames through pipeline
    println!("\n📹 Step 4: Sending video frames through pipeline...");
    const NUM_FRAMES: usize = 3;

    for i in 0..NUM_FRAMES {
        let video_buffer = create_test_video_frame(i as u64);

        let chunk_request = StreamRequest {
            request: Some(StreamRequestType::Chunk(AudioChunk {
                sequence: i as u64,
                buffer: Some(video_buffer),
            })),
        };

        request_tx
            .send(chunk_request)
            .await
            .expect("Failed to send chunk");
        println!("  → Sent frame {} (2x2 RGB24)", i);
    }

    // Step 5: Receive processed video frames
    println!("\n📦 Step 5: Receiving processed video frames...");
    for i in 0..NUM_FRAMES {
        let response = timeout(Duration::from_secs(5), response_stream.message())
            .await
            .expect(&format!("Timeout waiting for frame {}", i))
            .expect("Stream error")
            .expect(&format!("No response for frame {}", i));

        match response.response {
            Some(StreamResponseType::Result(result)) => {
                println!("  ← Received frame {} result", i);

                // Verify we got video data back
                assert!(
                    result.buffer.is_some(),
                    "Frame {} missing buffer",
                    i
                );

                let buffer = result.buffer.unwrap();
                assert_eq!(buffer.format, 1, "Frame {} wrong format", i);
                assert_eq!(buffer.data.len(), 12, "Frame {} wrong data size", i);

                // Step 6: Verify VideoFlip processed the frame correctly
                println!("\n✅ Step 6: Verifying VideoFlip processing for frame {}...", i);

                // After vertical flip, the pixels should be reordered:
                // Original top row: red (255,0,0), green (0,255,0)
                // Original bottom row: blue (0,0,255), white (255,255,255)
                // Flipped top row: blue, white
                // Flipped bottom row: red, green

                let pixels = &buffer.data;
                assert_eq!(pixels[0..3], [0, 0, 255], "Frame {} pixel 0 (blue)", i);
                assert_eq!(
                    pixels[3..6],
                    [255, 255, 255],
                    "Frame {} pixel 1 (white)",
                    i
                );
                assert_eq!(pixels[6..9], [255, 0, 0], "Frame {} pixel 2 (red)", i);
                assert_eq!(pixels[9..12], [0, 255, 0], "Frame {} pixel 3 (green)", i);

                println!("  ✓ Frame {} correctly flipped!", i);
                println!("    - Top row: blue, white (was bottom)");
                println!("    - Bottom row: red, green (was top)");

                // Verify sequence number
                assert_eq!(result.sequence, i as u64, "Frame {} wrong sequence", i);
                println!("  ✓ Sequence number correct: {}", result.sequence);

                // Verify execution metrics exist
                assert!(
                    result.execution_metrics.is_some(),
                    "Frame {} missing metrics",
                    i
                );
                let metrics = result.execution_metrics.unwrap();
                println!(
                    "  ✓ Execution time: {:.2}ms",
                    metrics.total_time_ms
                );
            }
            Some(StreamResponseType::Metrics(metrics)) => {
                println!("  📊 Received metrics update");
                println!("    - Chunks processed: {}", metrics.chunks_processed);
                println!("    - Avg latency: {:.2}ms", metrics.avg_latency_ms);
            }
            _ => panic!("Unexpected response type for frame {}", i),
        }
    }

    // Step 7: Close session gracefully
    println!("\n🛑 Step 7: Closing session...");
    let close_request = StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: 1, // CLOSE
        })),
    };

    request_tx
        .send(close_request)
        .await
        .expect("Failed to send close");

    // Wait for StreamClosed
    let close_response = timeout(Duration::from_secs(5), response_stream.message())
        .await
        .expect("Timeout waiting for StreamClosed")
        .expect("Stream error");

    if let Some(response) = close_response {
        match response.response {
            Some(StreamResponseType::Closed(closed)) => {
                println!("  ✓ Session closed: {}", closed.session_id);
                println!("  ✓ Reason: {}", closed.reason);
                if let Some(final_metrics) = closed.final_metrics {
                    println!(
                        "  ✓ Final metrics: {} chunks, {:.2}ms avg latency",
                        final_metrics.chunks_processed, final_metrics.avg_latency_ms
                    );
                }
            }
            _ => panic!("Expected StreamClosed response"),
        }
    }

    println!("\n🎉 All gRPC streaming E2E tests passed!");
    println!("\n✅ Validated complete flow:");
    println!("   1. Server startup ✓");
    println!("   2. Client connection ✓");
    println!("   3. Session creation with manifest ✓");
    println!("   4. Video frame transmission ✓");
    println!("   5. Pipeline processing (VideoFlip) ✓");
    println!("   6. Processed frame reception ✓");
    println!("   7. Graceful session closure ✓");
}

#[tokio::test]
async fn test_grpc_streaming_multiple_sessions() {
    println!("\n🧪 gRPC Multiple Sessions Test...\n");

    // Start server
    let (server_url, _server_handle) = start_test_server().await;
    println!("✓ Server started");

    // Create two concurrent sessions
    let session1 = tokio::spawn({
        let url = server_url.clone();
        async move {
            let mut client = StreamingPipelineServiceClient::connect(url)
                .await
                .unwrap();

            let (mut tx, rx) = tokio::sync::mpsc::channel(32);
            let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

            // Init session 1
            tx.send(StreamRequest {
                request: Some(StreamRequestType::Init(StreamInit {
                    client_version: "session1".to_string(),
                    manifest: Some(create_test_manifest()),
                    preview_features: vec![],
                })),
            })
            .await
            .unwrap();

            let mut response_stream = client.stream_pipeline(Request::new(stream)).await.unwrap().into_inner();

            // Wait for ready
            let ready = response_stream.message().await.unwrap().unwrap();
            assert!(matches!(
                ready.response,
                Some(StreamResponseType::Ready(_))
            ));

            // Send one frame
            tx.send(StreamRequest {
                request: Some(StreamRequestType::Chunk(AudioChunk {
                    sequence: 0,
                    buffer: Some(create_test_video_frame(0)),
                })),
            })
            .await
            .unwrap();

            // Receive result
            let result = response_stream.message().await.unwrap().unwrap();
            assert!(matches!(
                result.response,
                Some(StreamResponseType::Result(_))
            ));

            "session1 complete"
        }
    });

    let session2 = tokio::spawn({
        let url = server_url.clone();
        async move {
            let mut client = StreamingPipelineServiceClient::connect(url)
                .await
                .unwrap();

            let (mut tx, rx) = tokio::sync::mpsc::channel(32);
            let stream = tokio_stream::wrappers::ReceiverStream::new(rx);

            // Init session 2
            tx.send(StreamRequest {
                request: Some(StreamRequestType::Init(StreamInit {
                    client_version: "session2".to_string(),
                    manifest: Some(create_test_manifest()),
                    preview_features: vec![],
                })),
            })
            .await
            .unwrap();

            let mut response_stream = client.stream_pipeline(Request::new(stream)).await.unwrap().into_inner();

            // Wait for ready
            let ready = response_stream.message().await.unwrap().unwrap();
            assert!(matches!(
                ready.response,
                Some(StreamResponseType::Ready(_))
            ));

            // Send one frame
            tx.send(StreamRequest {
                request: Some(StreamRequestType::Chunk(AudioChunk {
                    sequence: 0,
                    buffer: Some(create_test_video_frame(0)),
                })),
            })
            .await
            .unwrap();

            // Receive result
            let result = response_stream.message().await.unwrap().unwrap();
            assert!(matches!(
                result.response,
                Some(StreamResponseType::Result(_))
            ));

            "session2 complete"
        }
    });

    // Wait for both sessions
    let result1 = session1.await.unwrap();
    let result2 = session2.await.unwrap();

    println!("✓ {}", result1);
    println!("✓ {}", result2);
    println!("\n🎉 Multiple concurrent sessions test passed!");
}

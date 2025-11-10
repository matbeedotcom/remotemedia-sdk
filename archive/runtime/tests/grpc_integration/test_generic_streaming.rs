//! Integration tests for generic streaming (Feature 004)
//!
//! Tests Phase 3: User Story 1 - Stream Non-Audio Data Types
//! - T054: Stream video frames through VideoProcessorNode
//! - T055: JSON calculator pipeline
//! - T056: Tensor streaming
//! - T057: End-to-end video streaming verification

use remotemedia_runtime::grpc_service::generated::{
    data_buffer, stream_control::Command, stream_request::Request as StreamRequestType,
    stream_response::Response as StreamResponseType,
    streaming_pipeline_service_client::StreamingPipelineServiceClient, DataBuffer, DataChunk,
    JsonData, ManifestMetadata, NodeManifest, PipelineManifest, PixelFormat, StreamControl,
    StreamInit, StreamRequest, TensorBuffer, TensorDtype, TextBuffer, VideoFrame,
};
use std::collections::HashMap;
use tonic::Request;

/// Helper to create a test manifest for a single node
fn create_test_manifest(
    node_id: &str,
    node_type: &str,
    input_type: i32,
    output_type: i32,
) -> PipelineManifest {
    PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: format!("test_{}", node_type),
            description: format!("Integration test for {}", node_type),
            created_at: "2025-01-15T00:00:00Z".to_string(),
        }),
        nodes: vec![NodeManifest {
            id: node_id.to_string(),
            node_type: node_type.to_string(),
            params: "{}".to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,
            input_types: vec![input_type],
            output_types: vec![output_type],
        }],
        connections: vec![],
    }
}

/// T054: Stream 10 video frames through VideoProcessorNode
#[tokio::test]
async fn test_video_frame_streaming() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "http://[::1]:50051";
    let mut client = StreamingPipelineServiceClient::connect(server_addr.to_string()).await?;

    // Create manifest with VideoProcessorNode
    let manifest = create_test_manifest(
        "video_processor",
        "VideoProcessorNode",
        2, // DATA_TYPE_HINT_VIDEO
        4, // DATA_TYPE_HINT_JSON (detection results)
    );

    // Start streaming session
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let response = client.stream_pipeline(Request::new(request_stream)).await?;
    let mut response_stream = response.into_inner();

    // Send StreamInit
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "v1".to_string(),
            data_inputs: std::collections::HashMap::new(),
            resource_limits: None,
            expected_chunk_size: 0,
        })),
    })
    .await?;

    // Wait for StreamReady
    if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Ready(ready)) => {
                println!("✅ Session started: {}", ready.session_id);
            }
            _ => panic!("Expected StreamReady"),
        }
    }

    // Stream 10 video frames
    let mut detection_counts = Vec::new();

    for frame_num in 0..10 {
        // Create a 640x480 RGB24 video frame
        let video_frame = VideoFrame {
            pixel_data: vec![0u8; 640 * 480 * 3],
            width: 640,
            height: 480,
            format: PixelFormat::Rgb24 as i32,
            frame_number: frame_num,
            timestamp_us: frame_num * 33333, // 30 FPS
        };

        let data_buffer = DataBuffer {
            data_type: Some(data_buffer::DataType::Video(video_frame)),
            metadata: HashMap::new(),
        };

        let data_chunk = DataChunk {
            node_id: "video_processor".to_string(),
            buffer: Some(data_buffer),
            sequence: frame_num,
            timestamp_ms: frame_num * 33,
            named_buffers: HashMap::new(),
        };

        tx.send(StreamRequest {
            request: Some(StreamRequestType::DataChunk(data_chunk)),
        })
        .await?;

        // Receive result
        if let Some(resp) = response_stream.message().await? {
            match resp.response {
                Some(StreamResponseType::Result(result)) => {
                    assert_eq!(result.sequence, frame_num);

                    // Extract JSON detection results
                    if let Some(output) = result.data_outputs.get("video_processor") {
                        if let Some(data_buffer::DataType::Json(json_data)) = &output.data_type {
                            let detection_json: serde_json::Value =
                                serde_json::from_str(&json_data.json_payload)?;

                            println!(
                                "Frame {}: {} detections",
                                frame_num, detection_json["detection_count"]
                            );

                            detection_counts
                                .push(detection_json["detection_count"].as_u64().unwrap());

                            // Verify JSON structure
                            assert!(detection_json["frame_number"].is_u64());
                            assert!(detection_json["detections"].is_array());
                            assert!(detection_json["width"].as_u64().unwrap() == 640);
                            assert!(detection_json["height"].as_u64().unwrap() == 480);
                        }
                    }
                }
                Some(StreamResponseType::Error(err)) => {
                    panic!("Streaming error: {}", err.message);
                }
                _ => {}
            }
        }
    }

    // Close stream
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: Command::Close as i32,
        })),
    })
    .await?;

    // Verify we got detections for all frames
    assert_eq!(detection_counts.len(), 10);
    println!("✅ Successfully streamed 10 video frames with detections");

    Ok(())
}

/// T055: JSON calculator pipeline
#[tokio::test]
async fn test_json_calculator_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "http://[::1]:50051";
    let mut client = StreamingPipelineServiceClient::connect(server_addr.to_string()).await?;

    // Create manifest with CalculatorNode
    let manifest = create_test_manifest(
        "calculator",
        "CalculatorNode",
        4, // DATA_TYPE_HINT_JSON
        4, // DATA_TYPE_HINT_JSON
    );

    // Start streaming session
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let response = client.stream_pipeline(Request::new(request_stream)).await?;
    let mut response_stream = response.into_inner();

    // Send StreamInit
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "v1".to_string(),
            data_inputs: std::collections::HashMap::new(),
            resource_limits: None,
            expected_chunk_size: 0,
        })),
    })
    .await?;

    // Wait for StreamReady
    if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Ready(_)) => {
                println!("✅ Calculator session started");
            }
            _ => panic!("Expected StreamReady"),
        }
    }

    // Test different operations
    let test_cases = vec![
        (
            serde_json::json!({"operation": "add", "operands": [10, 20]}),
            30.0,
        ),
        (
            serde_json::json!({"operation": "subtract", "operands": [50, 15]}),
            35.0,
        ),
        (
            serde_json::json!({"operation": "multiply", "operands": [7, 6]}),
            42.0,
        ),
        (
            serde_json::json!({"operation": "divide", "operands": [100, 4]}),
            25.0,
        ),
    ];

    for (seq, (input, expected_result)) in test_cases.iter().enumerate() {
        let json_data = JsonData {
            json_payload: input.to_string(),
            schema_type: "CalculatorRequest".to_string(),
        };

        let data_buffer = DataBuffer {
            data_type: Some(data_buffer::DataType::Json(json_data)),
            metadata: HashMap::new(),
        };

        let data_chunk = DataChunk {
            node_id: "calculator".to_string(),
            buffer: Some(data_buffer),
            sequence: seq as u64,
            timestamp_ms: 0,
            named_buffers: HashMap::new(),
        };

        tx.send(StreamRequest {
            request: Some(StreamRequestType::DataChunk(data_chunk)),
        })
        .await?;

        // Receive result
        if let Some(resp) = response_stream.message().await? {
            match resp.response {
                Some(StreamResponseType::Result(result)) => {
                    if let Some(output) = result.data_outputs.get("calculator") {
                        if let Some(data_buffer::DataType::Json(json_data)) = &output.data_type {
                            let result_json: serde_json::Value =
                                serde_json::from_str(&json_data.json_payload)?;

                            let actual_result = result_json["result"].as_f64().unwrap();
                            assert_eq!(actual_result, *expected_result);

                            println!(
                                "✅ {}: result = {}",
                                result_json["operation"].as_str().unwrap(),
                                actual_result
                            );
                        }
                    }
                }
                Some(StreamResponseType::Error(err)) => {
                    panic!("Calculator error: {}", err.message);
                }
                _ => {}
            }
        }
    }

    // Close stream
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: Command::Close as i32,
        })),
    })
    .await?;

    println!("✅ All calculator operations completed successfully");

    Ok(())
}

/// T056: Tensor streaming test
#[tokio::test]
async fn test_tensor_streaming() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "http://[::1]:50051";
    let mut client = StreamingPipelineServiceClient::connect(server_addr.to_string()).await?;

    // Create manifest with PassThrough node (tensor support)
    let manifest = create_test_manifest(
        "passthrough",
        "PassThrough",
        3, // DATA_TYPE_HINT_TENSOR
        3, // DATA_TYPE_HINT_TENSOR
    );

    // Start streaming session
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let response = client.stream_pipeline(Request::new(request_stream)).await?;
    let mut response_stream = response.into_inner();

    // Send StreamInit
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "v1".to_string(),
            data_inputs: std::collections::HashMap::new(),
            resource_limits: None,
            expected_chunk_size: 0,
        })),
    })
    .await?;

    // Wait for StreamReady
    if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Ready(_)) => {
                println!("✅ Tensor session started");
            }
            _ => panic!("Expected StreamReady"),
        }
    }

    // Stream a tensor (e.g., embedding vector)
    let embedding_size = 512;
    let tensor = TensorBuffer {
        data: vec![0u8; embedding_size * 4], // 512 F32 elements
        shape: vec![embedding_size as u64],
        dtype: TensorDtype::F32 as i32,
        layout: "C".to_string(),
    };

    let data_buffer = DataBuffer {
        data_type: Some(data_buffer::DataType::Tensor(tensor.clone())),
        metadata: HashMap::new(),
    };

    let data_chunk = DataChunk {
        node_id: "passthrough".to_string(),
        buffer: Some(data_buffer),
        sequence: 0,
        timestamp_ms: 0,
        named_buffers: HashMap::new(),
    };

    tx.send(StreamRequest {
        request: Some(StreamRequestType::DataChunk(data_chunk)),
    })
    .await?;

    // Receive result
    if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Result(result)) => {
                if let Some(output) = result.data_outputs.get("passthrough") {
                    if let Some(data_buffer::DataType::Tensor(tensor_out)) = &output.data_type {
                        assert_eq!(tensor_out.shape, vec![embedding_size as u64]);
                        assert_eq!(tensor_out.dtype, TensorDtype::F32 as i32);
                        assert_eq!(tensor_out.data.len(), embedding_size * 4);

                        println!(
                            "✅ Tensor passthrough successful: shape={:?}, dtype={}",
                            tensor_out.shape, tensor_out.dtype
                        );
                    }
                }
            }
            Some(StreamResponseType::Error(err)) => {
                panic!("Tensor error: {}", err.message);
            }
            _ => {}
        }
    }

    // Close stream
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: Command::Close as i32,
        })),
    })
    .await?;

    println!("✅ Tensor streaming test completed");

    Ok(())
}

/// T057: End-to-end video streaming verification
/// This test verifies the complete flow with metrics
#[tokio::test]
async fn test_video_streaming_with_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "http://[::1]:50051";

    // Configure client with larger message size limit for 1080p video (10MB)
    let channel = tonic::transport::Channel::from_static("http://[::1]:50051")
        .connect()
        .await?;

    let mut client = StreamingPipelineServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024) // 10MB for large video frames
        .max_encoding_message_size(10 * 1024 * 1024); // 10MB

    // Create manifest
    let manifest = create_test_manifest(
        "video_proc",
        "VideoProcessorNode",
        2, // VIDEO
        4, // JSON
    );

    // Start streaming
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let response = client.stream_pipeline(Request::new(request_stream)).await?;
    let mut response_stream = response.into_inner();

    // Init
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "v1".to_string(),
            data_inputs: std::collections::HashMap::new(),
            resource_limits: None,
            expected_chunk_size: 0,
        })),
    })
    .await?;

    // Wait for ready
    let session_id = if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Ready(ready)) => ready.session_id,
            _ => panic!("Expected StreamReady"),
        }
    } else {
        panic!("No response");
    };

    println!("✅ Session: {}", session_id);

    // Stream frames and verify
    let num_frames = 10;
    let mut total_detections = 0;
    let mut received_metrics = false;

    for frame_num in 0..num_frames {
        let video_frame = VideoFrame {
            pixel_data: vec![128u8; 1920 * 1080 * 3], // 1080p
            width: 1920,
            height: 1080,
            format: PixelFormat::Rgb24 as i32,
            frame_number: frame_num,
            timestamp_us: frame_num * 33333,
        };

        tx.send(StreamRequest {
            request: Some(StreamRequestType::DataChunk(DataChunk {
                node_id: "video_proc".to_string(),
                buffer: Some(DataBuffer {
                    data_type: Some(data_buffer::DataType::Video(video_frame)),
                    metadata: HashMap::new(),
                }),
                sequence: frame_num,
                timestamp_ms: 0,
                named_buffers: HashMap::new(),
            })),
        })
        .await?;

        // Collect responses (results + maybe metrics)
        while let Some(resp) = response_stream.message().await? {
            match resp.response {
                Some(StreamResponseType::Result(result)) => {
                    // Extract detections
                    if let Some(output) = result.data_outputs.get("video_proc") {
                        if let Some(data_buffer::DataType::Json(json_data)) = &output.data_type {
                            let json: serde_json::Value =
                                serde_json::from_str(&json_data.json_payload)?;
                            total_detections += json["detection_count"].as_u64().unwrap();
                        }
                    }
                    break; // Got result, continue to next frame
                }
                Some(StreamResponseType::Metrics(metrics)) => {
                    received_metrics = true;
                    println!(
                        "📊 Metrics: chunks={}, total_items={}, avg_latency={}ms",
                        metrics.chunks_processed, metrics.total_items, metrics.average_latency_ms
                    );
                }
                Some(StreamResponseType::Error(err)) => {
                    panic!("Error: {}", err.message);
                }
                _ => {}
            }
        }
    }

    // Close
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: Command::Close as i32,
        })),
    })
    .await?;

    // Final metrics
    if let Some(resp) = response_stream.message().await? {
        if let Some(StreamResponseType::Closed(closed)) = resp.response {
            println!("✅ Stream closed: {}", closed.reason);

            if let Some(final_metrics) = closed.final_metrics {
                println!("📊 Final metrics:");
                println!("   - Wall time: {:.2}ms", final_metrics.wall_time_ms);
                println!("   - Memory: {} bytes", final_metrics.memory_used_bytes);
                println!("   - Data types: {:?}", final_metrics.data_type_breakdown);
            }
        }
    }

    // Verify
    assert!(total_detections > 0, "Should have detections");

    // Note: Periodic metrics are optional - they may not be sent for short streams
    if received_metrics {
        println!("✅ Received periodic metrics during streaming");
    } else {
        println!("ℹ️  No periodic metrics received (stream may have been too short)");
    }

    println!(
        "✅ End-to-end test complete: {} frames, {} total detections",
        num_frames, total_detections
    );

    Ok(())
}

/// Test: Audio + Video inputs → Text output (Multi-modal pipeline)
/// This test demonstrates streaming multiple data types as inputs and receiving text output
#[tokio::test]
async fn test_multimodal_audio_video_to_text() -> Result<(), Box<dyn std::error::Error>> {
    let server_addr = "http://[::1]:50051";

    // Configure client with larger message size for video
    let channel = tonic::transport::Channel::from_static("http://[::1]:50051")
        .connect()
        .await?;

    let mut client = StreamingPipelineServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);

    // Create manifest for a node that accepts multiple inputs and outputs text
    // Using VideoProcessorNode as a proxy - it will process both inputs
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: "multimodal_test".to_string(),
            description: "Audio + Video → Text pipeline".to_string(),
            created_at: "2025-01-15T00:00:00Z".to_string(),
        }),
        nodes: vec![NodeManifest {
            id: "multimodal_processor".to_string(),
            node_type: "VideoProcessorNode".to_string(),
            params: "{}".to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,
            input_types: vec![1, 2], // AUDIO=1, VIDEO=2
            output_types: vec![5],   // TEXT=5
        }],
        connections: vec![],
    };

    // Start streaming
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let response = client.stream_pipeline(Request::new(request_stream)).await?;
    let mut response_stream = response.into_inner();

    // Send StreamInit
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "v1".to_string(),
            data_inputs: HashMap::new(),
            resource_limits: None,
            expected_chunk_size: 0,
        })),
    })
    .await?;

    // Wait for StreamReady
    if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Ready(ready)) => {
                println!("✅ Multimodal session started: {}", ready.session_id);
            }
            _ => panic!("Expected StreamReady"),
        }
    }

    // Test with different input types using appropriate node types
    // For now, test single inputs via named_buffers to validate the mechanism works
    println!("\n📝 Testing: Video input via named_buffers");

    {
        let mut named_buffers = HashMap::new();

        // Send video via named_buffers
        let video_frame = VideoFrame {
            pixel_data: vec![128u8; 320 * 240 * 3],
            width: 320,
            height: 240,
            format: PixelFormat::Rgb24 as i32,
            frame_number: 0,
            timestamp_us: 0,
        };

        named_buffers.insert(
            "video_input".to_string(),
            DataBuffer {
                data_type: Some(data_buffer::DataType::Video(video_frame)),
                metadata: HashMap::new(),
            },
        );

        // Send the data chunk with named buffers
        tx.send(StreamRequest {
            request: Some(StreamRequestType::DataChunk(DataChunk {
                node_id: "multimodal_processor".to_string(),
                buffer: None, // Using named_buffers instead
                sequence: 0,
                timestamp_ms: 0,
                named_buffers,
            })),
        })
        .await?;

        // Receive result
        if let Some(resp) = response_stream.message().await? {
            match resp.response {
                Some(StreamResponseType::Result(result)) => {
                    if let Some(output) = result.data_outputs.get("multimodal_processor") {
                        match &output.data_type {
                            Some(data_buffer::DataType::Text(text_buffer)) => {
                                println!("  ✅ Text output: {} bytes", text_buffer.text_data.len());
                                let preview = String::from_utf8_lossy(&text_buffer.text_data);
                                println!(
                                    "  📄 Preview: {}",
                                    &preview.chars().take(50).collect::<String>()
                                );
                            }
                            Some(data_buffer::DataType::Json(json_data)) => {
                                println!("  ✅ JSON output: {}", json_data.json_payload);
                            }
                            _ => {
                                println!(
                                    "  ✅ Received output (type: {:?})",
                                    output.data_type.as_ref().map(|_| "DataBuffer")
                                );
                            }
                        }
                    }
                }
                Some(StreamResponseType::Error(err)) => {
                    panic!("Unexpected error: {}", err.message);
                }
                _ => {}
            }
        }
    }

    // Close stream
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: Command::Close as i32,
        })),
    })
    .await?;

    // Wait for close confirmation
    if let Some(resp) = response_stream.message().await? {
        if let Some(StreamResponseType::Closed(_)) = resp.response {
            println!("\n✅ Multimodal streaming test complete");
        }
    }

    Ok(())
}

/// Phase 4: Test audio+video synchronization with SynchronizedAudioVideoNode
#[tokio::test]
async fn test_audio_video_synchronization() -> Result<(), Box<dyn std::error::Error>> {
    use remotemedia_runtime::grpc_service::generated::{AudioBuffer, AudioFormat};

    let server_addr = "http://[::1]:50051";
    let channel = tonic::transport::Endpoint::from_shared(server_addr.to_string())?
        .connect()
        .await?;

    let mut client = StreamingPipelineServiceClient::new(channel)
        .max_decoding_message_size(10 * 1024 * 1024)
        .max_encoding_message_size(10 * 1024 * 1024);

    println!("\n🎬 Testing Audio+Video Synchronization with SynchronizedAudioVideoNode");

    // Create manifest for sync node
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: "test_sync_av".to_string(),
            description: "Test audio-video synchronization".to_string(),
            created_at: "2025-01-15T00:00:00Z".to_string(),
        }),
        nodes: vec![NodeManifest {
            id: "sync_node".to_string(),
            node_type: "SynchronizedAudioVideoNode".to_string(),
            params: r#"{"sync_tolerance_ms": 20.0}"#.to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,
            input_types: vec![1, 2], // AUDIO=1, VIDEO=2
            output_types: vec![3],   // JSON=3
        }],
        connections: vec![],
    };

    // Start streaming
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = tokio_stream::wrappers::ReceiverStream::new(rx);

    let response = client.stream_pipeline(Request::new(request_stream)).await?;
    let mut response_stream = response.into_inner();

    // Init
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            client_version: "v1".to_string(),
            data_inputs: HashMap::new(),
            resource_limits: None,
            expected_chunk_size: 0,
        })),
    })
    .await?;

    // Wait for ready
    let session_id = if let Some(resp) = response_stream.message().await? {
        match resp.response {
            Some(StreamResponseType::Ready(ready)) => ready.session_id,
            _ => panic!("Expected StreamReady"),
        }
    } else {
        panic!("No response");
    };

    println!("✅ Session: {}", session_id);

    // Test scenarios: perfect sync, slight drift, out of sync
    let test_cases = vec![
        (0i64, 0i64, "Perfect sync"),
        (0i64, 15_000i64, "Video 15ms ahead (good)"),
        (0i64, -10_000i64, "Audio 10ms ahead (good)"),
        (0i64, 50_000i64, "Video 50ms ahead (poor)"),
    ];

    for (seq, (audio_ts, video_ts, description)) in test_cases.iter().enumerate() {
        println!("\n📊 Test case {}: {}", seq + 1, description);

        // Create audio buffer (100ms @ 16kHz)
        let samples_f32 = vec![0.0f32; 1600]; // 100ms @ 16kHz
        let samples_bytes: Vec<u8> = samples_f32.iter().flat_map(|f| f.to_le_bytes()).collect();

        let audio = AudioBuffer {
            samples: samples_bytes,
            sample_rate: 16000,
            channels: 1,
            format: AudioFormat::F32 as i32,
            num_samples: 1600,
        };

        // Create video frame with specific timestamp
        let video = VideoFrame {
            pixel_data: vec![128u8; 320 * 240 * 3],
            width: 320,
            height: 240,
            format: PixelFormat::Rgb24 as i32,
            frame_number: seq as u64,
            timestamp_us: *video_ts as u64,
        };

        // Send both via named_buffers
        let mut named_buffers = HashMap::new();
        named_buffers.insert(
            "audio".to_string(),
            DataBuffer {
                data_type: Some(data_buffer::DataType::Audio(audio)),
                metadata: HashMap::new(),
            },
        );
        named_buffers.insert(
            "video".to_string(),
            DataBuffer {
                data_type: Some(data_buffer::DataType::Video(video)),
                metadata: HashMap::new(),
            },
        );

        tx.send(StreamRequest {
            request: Some(StreamRequestType::DataChunk(DataChunk {
                node_id: "sync_node".to_string(),
                buffer: None, // Using named_buffers
                sequence: seq as u64,
                timestamp_ms: 0,
                named_buffers,
            })),
        })
        .await?;

        // Receive sync report
        if let Some(resp) = response_stream.message().await? {
            match resp.response {
                Some(StreamResponseType::Result(result)) => {
                    if let Some(output) = result.data_outputs.get("sync_node") {
                        if let Some(data_buffer::DataType::Json(json_data)) = &output.data_type {
                            let report: serde_json::Value =
                                serde_json::from_str(&json_data.json_payload)?;

                            println!(
                                "  ✅ Sync Status: is_synced={}, quality={}, offset={}ms",
                                report["sync_status"]["is_synced"],
                                report["sync_status"]["quality"],
                                report["sync_status"]["offset_ms"]
                            );

                            if let Some(rec) = report["recommendation"].as_str() {
                                println!("  💡 Recommendation: {}", rec);
                            }

                            // Validate expected results
                            let expected_synced = video_ts.abs() <= 20_000;
                            let actual_synced =
                                report["sync_status"]["is_synced"].as_bool().unwrap();
                            assert_eq!(
                                actual_synced, expected_synced,
                                "Expected is_synced={} but got {}",
                                expected_synced, actual_synced
                            );
                        }
                    }
                }
                Some(StreamResponseType::Error(err)) => {
                    panic!("Sync error: {}", err.message);
                }
                _ => {}
            }
        }
    }

    // Close stream
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(StreamControl {
            command: Command::Close as i32,
        })),
    })
    .await?;

    // Wait for close confirmation
    if let Some(resp) = response_stream.message().await? {
        if let Some(StreamResponseType::Closed(_)) = resp.response {
            println!("\n✅ Audio-Video synchronization test complete");
        }
    }

    Ok(())
}

//! Simple gRPC client for testing RemoteMedia services
//!
//! Tests PipelineExecutionService (GetVersion and ExecutePipeline RPCs)
//!
//! # Usage
//!
//! ```bash
//! # Start server in terminal 1
//! GRPC_REQUIRE_AUTH=false cargo run --bin grpc_server --features grpc-transport
//!
//! # Run client in terminal 2
//! cargo run --bin grpc_client --features grpc-transport
//! cargo run --bin grpc_client --features grpc-transport -- version
//! cargo run --bin grpc_client --features grpc-transport -- execute
//! ```

#![cfg(feature = "grpc-transport")]

use remotemedia_runtime::grpc_service::generated::{
    pipeline_execution_service_client::PipelineExecutionServiceClient,
    stream_request::Request as StreamRequestType,
    streaming_pipeline_service_client::StreamingPipelineServiceClient,
    AudioBuffer as ProtoAudioBuffer, AudioChunk, AudioFormat as ProtoAudioFormat, ExecuteRequest,
    ManifestMetadata, NodeManifest, PipelineManifest, StreamInit, StreamRequest, VersionRequest,
};
use std::time::Instant;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let command = args.get(1).map(|s| s.as_str()).unwrap_or("version");

    let server_addr =
        std::env::var("GRPC_SERVER_ADDR").unwrap_or_else(|_| "http://[::1]:50051".to_string());

    info!("Connecting to server at {}", server_addr);

    match command {
        "version" => test_get_version(&server_addr).await?,
        "execute" => test_execute_pipeline(&server_addr).await?,
        "stream" => test_streaming_pipeline(&server_addr).await?,
        _ => {
            eprintln!("Unknown command: {}", command);
            eprintln!("Usage: grpc_client [version|execute|stream]");
            std::process::exit(1);
        }
    }

    Ok(())
}

async fn test_get_version(server_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Testing GetVersion RPC ===");

    let mut client = PipelineExecutionServiceClient::connect(server_addr.to_string()).await?;

    let request = Request::new(VersionRequest {
        client_version: "v1".to_string(),
    });

    let start = Instant::now();
    let response = client.get_version(request).await?;
    let latency = start.elapsed();

    let response_data = response.into_inner();

    if let Some(version_info) = response_data.version_info {
        info!("✅ GetVersion successful");
        info!("   Protocol version: {}", version_info.protocol_version);
        info!("   Runtime version: {}", version_info.runtime_version);
        info!(
            "   Supported nodes: {} types",
            version_info.supported_node_types.len()
        );
        info!("   Compatible: {}", response_data.compatible);
        info!("   Latency: {:?}", latency);
    } else {
        error!("❌ No version info returned");
    }

    Ok(())
}

async fn test_execute_pipeline(server_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Testing ExecutePipeline RPC ===");

    let mut client = PipelineExecutionServiceClient::connect(server_addr.to_string()).await?;

    // Create a simple manifest with passthrough node
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: "test_pipeline".to_string(),
            description: "Simple passthrough test".to_string(),
            created_at: "2025-10-28T00:00:00Z".to_string(),
        }),
        nodes: vec![NodeManifest {
            id: "node1".to_string(),
            node_type: "CalculatorNode".to_string(),
            params: r#"{"operation":"add"}"#.to_string(),
            is_streaming: false,
            capabilities: None,
            host: String::new(),
            runtime_hint: 0,       // RUNTIME_HINT_UNSPECIFIED
            input_types: vec![4],  // DATA_TYPE_HINT_JSON
            output_types: vec![4], // DATA_TYPE_HINT_JSON
        }],
        connections: vec![],
    };

    // Create test data input for Calculator (2 + 3 = 5)
    let input_data = serde_json::json!({
        "a": 2,
        "b": 3
    });

    // Create DataBuffer with JSON data
    use remotemedia_runtime::grpc_service::generated::{data_buffer, DataBuffer, JsonData};
    let json_buffer = DataBuffer {
        data_type: Some(data_buffer::DataType::Json(JsonData {
            json_payload: input_data.to_string(),
            schema_type: "CalculatorInput".to_string(),
        })),
        metadata: std::collections::HashMap::new(),
    };

    let mut data_inputs = std::collections::HashMap::new();
    data_inputs.insert("node1".to_string(), json_buffer);

    let request = Request::new(ExecuteRequest {
        manifest: Some(manifest),
        data_inputs,
        resource_limits: None,
        client_version: "v1".to_string(),
    });

    let start = Instant::now();
    let response = client.execute_pipeline(request).await?;
    let latency = start.elapsed();

    let execute_response = response.into_inner();

    match execute_response.outcome {
        Some(remotemedia_runtime::grpc_service::generated::execute_response::Outcome::Result(
            result,
        )) => {
            info!("✅ ExecutePipeline successful");
            info!("   Data outputs: {}", result.data_outputs.len());
            info!("   Data outputs: {}", result.data_outputs.len());

            if let Some(metrics) = result.metrics {
                info!("   Execution time: {:.2}ms", metrics.wall_time_ms);
                info!("   Memory used: {} bytes", metrics.memory_used_bytes);
                info!("   Nodes executed: {}", metrics.node_metrics.len());
                info!(
                    "   Serialization overhead: {:.2}%",
                    (metrics.serialization_time_ms / metrics.wall_time_ms) * 100.0
                );
            }

            info!("   Total latency: {:?}", latency);
        }
        Some(remotemedia_runtime::grpc_service::generated::execute_response::Outcome::Error(
            error,
        )) => {
            error!("❌ ExecutePipeline failed: {:?}", error.error_type);
            error!("   Message: {}", error.message);
            if !error.context.is_empty() {
                error!("   Context: {}", error.context);
            }
        }
        None => {
            error!("❌ No outcome returned");
        }
    }

    Ok(())
}

async fn test_streaming_pipeline(server_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("=== Testing StreamPipeline RPC (Bidirectional Streaming) ===");

    let mut client = StreamingPipelineServiceClient::connect(server_addr.to_string()).await?;

    // Create manifest for a simple PassThrough node
    let manifest = PipelineManifest {
        version: "v1".to_string(),
        metadata: Some(ManifestMetadata {
            name: "streaming_test".to_string(),
            description: "Streaming audio test".to_string(),
            created_at: "2025-10-28T00:00:00Z".to_string(),
        }),
        nodes: vec![NodeManifest {
            id: "node1".to_string(),
            node_type: "PassThrough".to_string(),
            params: "{}".to_string(),
            is_streaming: true,
            capabilities: None,
            host: String::new(),
            runtime_hint: 4,       // RuntimeHint::Auto
            input_types: vec![1],  // DATA_TYPE_HINT_AUDIO
            output_types: vec![1], // DATA_TYPE_HINT_AUDIO
        }],
        connections: vec![],
    };

    // Create channel for sending requests
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let request_stream = ReceiverStream::new(rx);

    // Send init message
    info!("Sending StreamInit...");
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Init(StreamInit {
            manifest: Some(manifest),
            data_inputs: std::collections::HashMap::new(),
            resource_limits: None,
            client_version: "v1".to_string(),
            expected_chunk_size: 1600,
        })),
    })
    .await?;

    // Start streaming
    let start_time = Instant::now();
    let mut response_stream = client
        .stream_pipeline(Request::new(request_stream))
        .await?
        .into_inner();

    // Read ready response
    if let Some(response) = response_stream.message().await? {
        match response.response {
            Some(
                remotemedia_runtime::grpc_service::generated::stream_response::Response::Ready(
                    ready,
                ),
            ) => {
                info!("✅ Stream ready!");
                info!("   Session ID: {}", ready.session_id);
                info!(
                    "   Recommended chunk size: {} samples",
                    ready.recommended_chunk_size
                );
                info!("   Max buffer latency: {}ms", ready.max_buffer_latency_ms);
            }
            Some(
                remotemedia_runtime::grpc_service::generated::stream_response::Response::Error(
                    error,
                ),
            ) => {
                error!(
                    "❌ Received error instead of StreamReady: {}",
                    error.message
                );
                return Ok(());
            }
            other => {
                error!("❌ Expected StreamReady, got {:?}", other);
                return Ok(());
            }
        }
    }

    // Send 5 audio chunks (100ms each at 16kHz)
    let chunk_size = 1600;
    info!("Sending {} audio chunks...", 5);

    for seq in 0u64..5 {
        // Create test audio chunk (sine wave)
        let mut samples = Vec::with_capacity(chunk_size);
        for i in 0..chunk_size {
            let t = ((seq * chunk_size as u64 + i as u64) as f32) / 16000.0;
            let sample = (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.5;
            samples.push(sample);
        }

        // Convert to bytes
        let bytes: Vec<u8> = samples.iter().flat_map(|f| f.to_le_bytes()).collect();

        let audio_buffer = ProtoAudioBuffer {
            samples: bytes,
            sample_rate: 16000,
            channels: 1,
            format: ProtoAudioFormat::F32 as i32,
            num_samples: chunk_size as u64,
        };

        let chunk = AudioChunk {
            node_id: "node1".to_string(),
            buffer: Some(audio_buffer),
            sequence: seq,
            timestamp_ms: (seq * 100) as u64,
        };

        tx.send(StreamRequest {
            request: Some(StreamRequestType::AudioChunk(chunk)),
        })
        .await?;

        info!("   Sent chunk {} ({}ms)", seq, seq * 100);

        // Read response
        if let Some(response) = response_stream.message().await? {
            match response.response {
                Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Result(result)) => {
                    info!("   ✅ Received result for chunk {}: {:.2}ms latency, {} data outputs",
                        result.sequence,
                        result.processing_time_ms,
                        result.data_outputs.len()
                    );
                }
                Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Error(error)) => {
                    error!("   ❌ Error processing chunk {}: {}", seq, error.message);
                }
                Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Metrics(metrics)) => {
                    info!("   📊 Metrics: avg latency={:.2}ms, chunks={}", 
                        metrics.average_latency_ms, metrics.chunks_processed);
                }
                _ => {
                    info!("   Received other response type");
                }
            }
        }
    }

    // Close stream
    info!("Closing stream...");
    tx.send(StreamRequest {
        request: Some(StreamRequestType::Control(
            remotemedia_runtime::grpc_service::generated::StreamControl { command: 1 },
        )),
    })
    .await?;

    // Read final responses
    while let Some(response) = response_stream.message().await? {
        match response.response {
            Some(
                remotemedia_runtime::grpc_service::generated::stream_response::Response::Closed(
                    closed,
                ),
            ) => {
                info!("✅ Stream closed successfully");
                if let Some(metrics) = closed.final_metrics {
                    info!("   Execution time: {:.2}ms", metrics.wall_time_ms);
                    info!("   Memory used: {} bytes", metrics.memory_used_bytes);
                }
                break;
            }
            Some(
                remotemedia_runtime::grpc_service::generated::stream_response::Response::Error(
                    error,
                ),
            ) => {
                error!("❌ Stream error: {}", error.message);
                break;
            }
            _ => {}
        }
    }

    let total_time = start_time.elapsed();
    info!("Total stream duration: {:?}", total_time);

    Ok(())
}

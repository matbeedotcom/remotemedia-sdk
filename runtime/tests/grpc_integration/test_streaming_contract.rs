// Test: StreamPipeline RPC protocol contract (T044)
//
// Verifies that StreamRequest/StreamResponse messages can be constructed
// and that the StreamInit → StreamReady flow works correctly.
//
// This test validates:
// - StreamRequest message types (StreamInit, AudioChunk, StreamControl)
// - StreamResponse message types (StreamReady, ChunkResult, StreamMetrics, StreamClosed)
// - First message must be StreamInit
// - StreamReady response contains session_id and buffer config

use remotemedia_runtime::grpc_service::generated::{
    AudioChunk, ChunkResult, StreamControl, StreamInit, StreamRequest, StreamResponse,
    StreamReady, StreamMetrics, StreamClosed, stream_control::Command,
    PipelineManifest, NodeManifest, AudioBuffer, AudioFormat, DataBuffer, data_buffer, JsonData,
};
use std::collections::HashMap;

// Unit test: Verify StreamRequest message construction
#[test]
fn test_stream_request_construction() {
    // Test StreamInit variant
    let manifest = PipelineManifest {
        version: "1.0.0".to_string(),
        metadata: None, // Optional
        nodes: vec![
            NodeManifest {
                id: "input".to_string(),
                node_type: "audio_input".to_string(),
                params: "{}".to_string(),
                is_streaming: true,
                capabilities: None, // Optional
                host: "".to_string(),
                runtime_hint: 0, // RUNTIME_HINT_UNSPECIFIED
                input_types: vec![],
                output_types: vec![],
            },
        ],
        connections: vec![],
    };

    let init = StreamInit {
        manifest: Some(manifest.clone()),
        data_inputs: HashMap::new(),
        resource_limits: None,
        client_version: "1.0.0".to_string(),
        expected_chunk_size: 4096,
    };

    let request = StreamRequest {
        request: Some(remotemedia_runtime::grpc_service::generated::stream_request::Request::Init(init)),
    };

    // Verify we can extract the init message
    match request.request {
        Some(remotemedia_runtime::grpc_service::generated::stream_request::Request::Init(i)) => {
            assert_eq!(i.client_version, "1.0.0");
            assert_eq!(i.expected_chunk_size, 4096);
            assert!(i.manifest.is_some());
        }
        _ => panic!("Expected Init variant"),
    }
}

// Unit test: Verify AudioChunk message construction
#[test]
fn test_audio_chunk_construction() {
    // Create audio buffer with F32 format (4 bytes per sample)
    // 1600 samples = 6400 bytes
    let samples_f32: Vec<f32> = vec![0.0; 1600];
    let samples_bytes: Vec<u8> = samples_f32.iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();

    let buffer = AudioBuffer {
        samples: samples_bytes,
        sample_rate: 16000,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: 1600, // 100ms at 16kHz mono
    };

    let chunk = AudioChunk {
        node_id: "input".to_string(),
        buffer: Some(buffer),
        sequence: 42,
        timestamp_ms: 4200,
    };

    let request = StreamRequest {
        request: Some(remotemedia_runtime::grpc_service::generated::stream_request::Request::AudioChunk(chunk)),
    };

    match request.request {
        Some(remotemedia_runtime::grpc_service::generated::stream_request::Request::AudioChunk(c)) => {
            assert_eq!(c.node_id, "input");
            assert_eq!(c.sequence, 42);
            assert_eq!(c.timestamp_ms, 4200);
            assert!(c.buffer.is_some());
            let buf = c.buffer.unwrap();
            assert_eq!(buf.num_samples, 1600);
            assert_eq!(buf.samples.len(), 6400); // 1600 samples * 4 bytes
        }
        _ => panic!("Expected AudioChunk variant"),
    }
}

// Unit test: Verify StreamControl message construction
#[test]
fn test_stream_control_construction() {
    // Test CLOSE command
    let control = StreamControl {
        command: Command::Close as i32,
    };

    let request = StreamRequest {
        request: Some(remotemedia_runtime::grpc_service::generated::stream_request::Request::Control(control)),
    };

    match request.request {
        Some(remotemedia_runtime::grpc_service::generated::stream_request::Request::Control(c)) => {
            assert_eq!(c.command, Command::Close as i32);
        }
        _ => panic!("Expected Control variant"),
    }

    // Test CANCEL command
    let cancel = StreamControl {
        command: Command::Cancel as i32,
    };
    assert_eq!(cancel.command, Command::Cancel as i32);
}

// Unit test: Verify StreamResponse message construction
#[test]
fn test_stream_response_construction() {
    // Test StreamReady variant
    let ready = StreamReady {
        session_id: "session-123".to_string(),
        recommended_chunk_size: 4096,
        max_buffer_latency_ms: 100,
    };

    let response = StreamResponse {
        response: Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Ready(ready)),
    };

    match response.response {
        Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Ready(r)) => {
            assert_eq!(r.session_id, "session-123");
            assert_eq!(r.recommended_chunk_size, 4096);
            assert_eq!(r.max_buffer_latency_ms, 100);
        }
        _ => panic!("Expected Ready variant"),
    }
}

// Unit test: Verify ChunkResult message construction
#[test]
fn test_chunk_result_construction() {
    let samples_f32: Vec<f32> = vec![0.0; 1600];
    let samples_bytes: Vec<u8> = samples_f32.iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();

    let buffer = AudioBuffer {
        samples: samples_bytes,
        sample_rate: 16000,
        channels: 1,
        format: AudioFormat::F32 as i32,
        num_samples: 1600,
    };

    let mut data_outputs = HashMap::new();
    data_outputs.insert(
        "vad".to_string(),
        DataBuffer {
            data_type: Some(data_buffer::DataType::Json(JsonData {
                json_payload: r#"{"has_speech": true}"#.to_string(),
                schema_type: String::new(),
            })),
            metadata: HashMap::new(),
        },
    );

    let result = ChunkResult {
        sequence: 42,
        data_outputs,
        processing_time_ms: 12.5,
        total_items_processed: 67200,
    };

    let response = StreamResponse {
        response: Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Result(result)),
    };

    match response.response {
        Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Result(r)) => {
            assert_eq!(r.sequence, 42);
            assert_eq!(r.processing_time_ms, 12.5);
            assert_eq!(r.total_items_processed, 67200);
            assert_eq!(r.data_outputs.len(), 1);
            assert_eq!(r.data_outputs.len(), 1);
        }
        _ => panic!("Expected Result variant"),
    }
}

// Unit test: Verify StreamMetrics message construction
#[test]
fn test_stream_metrics_construction() {
    let metrics = StreamMetrics {
        session_id: "session-123".to_string(),
        chunks_processed: 100,
        average_latency_ms: 35.2,
        total_items: 160000,
        buffer_items: 4096,
        chunks_dropped: 0,
        peak_memory_bytes: 1048576,
        data_type_breakdown: std::collections::HashMap::new(),
        cache_hits: 0,
        cache_misses: 0,
        cached_nodes_count: 0,
        cache_hit_rate: 0.0,
    };

    let response = StreamResponse {
        response: Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Metrics(metrics)),
    };

    match response.response {
        Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Metrics(m)) => {
            assert_eq!(m.session_id, "session-123");
            assert_eq!(m.chunks_processed, 100);
            assert_eq!(m.average_latency_ms, 35.2);
            assert_eq!(m.total_items, 160000);
            assert_eq!(m.buffer_items, 4096);
            assert_eq!(m.chunks_dropped, 0);
            assert_eq!(m.peak_memory_bytes, 1048576);
        }
        _ => panic!("Expected Metrics variant"),
    }
}

// Unit test: Verify StreamClosed message construction
#[test]
fn test_stream_closed_construction() {
    let closed = StreamClosed {
        session_id: "session-123".to_string(),
        final_metrics: None, // Would contain ExecutionMetrics in real scenario
        reason: "Client requested close".to_string(),
    };

    let response = StreamResponse {
        response: Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Closed(closed)),
    };

    match response.response {
        Some(remotemedia_runtime::grpc_service::generated::stream_response::Response::Closed(c)) => {
            assert_eq!(c.session_id, "session-123");
            assert_eq!(c.reason, "Client requested close");
        }
        _ => panic!("Expected Closed variant"),
    }
}

// Integration test: StreamInit → StreamReady flow (ignored until service implemented)
#[test]
#[ignore = "Requires StreamingPipelineService implementation"]
fn test_stream_init_flow() {
    // This will test:
    // 1. Client sends StreamInit with manifest
    // 2. Server validates and responds with StreamReady
    // 3. StreamReady contains valid session_id
    // 4. Client can then send AudioChunk messages
    todo!("Implement once StreamingPipelineService is available");
}

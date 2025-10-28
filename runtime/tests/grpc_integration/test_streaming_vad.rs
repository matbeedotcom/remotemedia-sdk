// Test: Streaming VAD pipeline (T045)
//
// Integration test that verifies streaming voice activity detection works correctly
// with 100ms audio chunks sent at 10 Hz.
//
// This test validates:
// - Client can stream AudioChunk messages in sequence
// - Server processes each chunk and returns ChunkResult
// - ChunkResults arrive in correct order (matching sequence numbers)
// - VAD data_outputs contain expected JSON structure

use remotemedia_runtime::grpc_service::generated::{
    AudioChunk, AudioBuffer, AudioFormat, ChunkResult,
    PipelineManifest, NodeManifest, StreamInit, StreamRequest,
};

// Integration test: Stream 100ms chunks through VAD pipeline
#[test]
#[ignore = "Requires StreamingPipelineService implementation"]
fn test_streaming_vad_processing() {
    // This test will:
    // 1. Initialize streaming pipeline with VAD node
    // 2. Stream 10 chunks of 100ms audio (1 second total)
    // 3. Verify each ChunkResult has correct sequence number
    // 4. Verify data_outputs contain VAD results with has_speech and confidence
    // 5. Verify processing happens in real-time (<50ms per chunk)
    
    // Example manifest:
    // {
    //   "version": "1.0.0",
    //   "nodes": [
    //     {"id": "input", "node_type": "audio_input", "is_streaming": true},
    //     {"id": "vad", "node_type": "VAD", "params": "{\"threshold\": 0.5}"}
    //   ],
    //   "connections": [{"from": "input", "to": "vad"}]
    // }
    
    todo!("Implement once StreamingPipelineService is available");
}

// Integration test: Verify sequence number ordering
#[test]
#[ignore = "Requires StreamingPipelineService implementation"]
fn test_chunk_sequence_ordering() {
    // This test will:
    // 1. Stream chunks with sequence numbers 0, 1, 2, ..., 9
    // 2. Verify ChunkResults arrive with matching sequence numbers
    // 3. Verify no chunks are dropped
    // 4. Verify no chunks are reordered
    
    todo!("Implement once StreamingPipelineService is available");
}

// Integration test: Verify data_outputs JSON structure
#[test]
#[ignore = "Requires StreamingPipelineService implementation"]
fn test_vad_output_structure() {
    // This test will:
    // 1. Stream audio chunks through VAD
    // 2. Parse data_outputs["vad"] as JSON
    // 3. Verify structure: {"has_speech": bool, "confidence": f32}
    // 4. Verify confidence values are in range [0.0, 1.0]
    
    todo!("Implement once StreamingPipelineService is available");
}

// Helper: Create 100ms audio chunk at 16kHz mono
#[allow(dead_code)]
fn create_audio_chunk(sequence: u64, sample_value: f32) -> AudioChunk {
    let num_samples = 1600; // 100ms at 16kHz
    let samples_f32: Vec<f32> = vec![sample_value; num_samples];
    let samples_bytes: Vec<u8> = samples_f32.iter()
        .flat_map(|&s| s.to_le_bytes())
        .collect();
    
    AudioChunk {
        node_id: "input".to_string(),
        buffer: Some(AudioBuffer {
            samples: samples_bytes,
            sample_rate: 16000,
            channels: 1,
            format: AudioFormat::F32 as i32,
            num_samples: num_samples as u64,
        }),
        sequence,
        timestamp_ms: sequence * 100, // 100ms per chunk
    }
}

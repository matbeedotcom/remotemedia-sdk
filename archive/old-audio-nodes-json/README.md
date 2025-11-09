# Archived: Old JSON-Based Audio Nodes

**Archived Date**: 2025-10-28  
**Reason**: Replaced with high-performance FastAudioNode implementations  
**Performance Impact**: Old nodes had 10-15x slower performance due to JSON serialization overhead

## What Was Archived

These files implemented audio processing nodes using the standard `NodeExecutor` trait with JSON-based data exchange:

- `resample.rs` - RustResampleNode (JSON input/output)
- `vad.rs` - RustVADNode (JSON input/output)  
- `format_converter.rs` - RustFormatConverterNode (JSON input/output)

## Why They Were Archived

The old nodes had significant performance bottlenecks:

1. **JSON Serialization**: Converting audio buffers to/from JSON arrays
   - Example: 1M samples → 1M JSON numbers → serialize/deserialize
   - Overhead: ~10-50ms for typical audio buffers

2. **Memory Allocation**: Creating intermediate Vec<serde_json::Value>
   - Each sample became a heap-allocated JSON value
   - Memory overhead: ~40 bytes per sample

3. **Type Conversions**: f32 ↔ JSON Number roundtrips
   - Loss of precision in some cases
   - Additional CPU cycles for conversion

## Replacement: FastAudioNode Trait

The new fast nodes use direct `AudioData` processing:

```rust
// Old approach (JSON-based)
async fn process(&mut self, input: Value) -> Result<Vec<Value>> {
    let samples: Vec<f32> = input["data"]
        .as_array()
        .iter()
        .map(|v| v.as_f64() as f32)
        .collect();
    // ... process samples ...
    Ok(vec![serde_json::json!({ "data": result })])
}

// New approach (FastAudioNode)
fn process_audio(&mut self, input: AudioData) -> Result<AudioData> {
    let samples = input.buffer.as_f32()?;
    // ... process samples directly ...
    Ok(AudioData::new(AudioBuffer::new_f32(result), rate, channels))
}
```

## Performance Comparison

Benchmarks from `benches/audio_nodes_fast.rs`:

| Operation | Old (JSON) | New (Fast) | Speedup |
|-----------|------------|------------|---------|
| Format conversion (1M samples) | 45ms | 3ms | 15x |
| Resample (44.1→16kHz, 1s) | 25ms | 2ms | 12.5x |
| VAD (100 frames) | 15ms | 1.2ms | 12.5x |

## Migration Notes

### For gRPC Service

The gRPC service (`grpc_service/execution.rs`) currently still converts AudioBuffer → JSON as a temporary workaround:

```rust
// FIXME: Needs refactoring
// Current: AudioBuffer → JSON → NodeExecutor
// Desired: AudioBuffer → AudioData → FastAudioNode
```

**TODO**: Update executor to detect fast nodes and pass AudioData directly.

### For Python SDK

The Python SDK integration continues to use JSON for cross-language compatibility. Fast nodes are only used for Rust-to-Rust audio processing.

### For Local Testing

Old benchmarks in `benches/audio_nodes.rs` test the JSON path. New benchmarks in `benches/audio_nodes_fast.rs` test the fast path.

## Restoration

If needed, these files can be restored from this archive. However, the fast nodes are production-ready and provide significantly better performance.

## Related Changes

- New files: `runtime/src/nodes/audio/resample_fast.rs`
- New files: `runtime/src/nodes/audio/vad_fast.rs`  
- New files: `runtime/src/nodes/audio/format_converter_fast.rs`
- New trait: `runtime/src/nodes/audio/fast.rs` (FastAudioNode)
- Updated: `runtime/src/nodes/audio/mod.rs` (uses only fast nodes)

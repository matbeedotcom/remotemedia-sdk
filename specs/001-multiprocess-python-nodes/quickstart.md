# Multiprocess Python Nodes - Quick Start Guide

## Overview

Enable concurrent execution of Python nodes with independent GILs using the `MultiprocessExecutor`. Each Python node runs in a separate process with zero-copy data transfer via iceoryx2 shared memory.

## Installation

```bash
# Build the runtime with multiprocess support
cargo build --features multiprocess

# Install Python client with multiprocess support
pip install remotemedia-sdk[multiprocess]
```

## Basic Usage

### 1. Configure the Runtime

```rust
// src/main.rs
use remotemedia_runtime::{Runtime, MultiprocessExecutor, MultiprocessConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create multiprocess executor for Python nodes
    let mp_executor = MultiprocessExecutor::new(MultiprocessConfig {
        max_processes_per_session: Some(10),
        channel_capacity: 100,
        init_timeout_secs: 30,
        python_executable: PathBuf::from("python"),
        enable_backpressure: true,
    });

    // Initialize runtime with executor
    let mut runtime = Runtime::builder()
        .register_executor("python", Box::new(mp_executor))
        .build()
        .await?;

    runtime.start().await?;
    Ok(())
}
```

### 2. Create a Python Node

```python
# nodes/my_audio_processor.py
from remotemedia.core.multiprocessing import MultiprocessNode, register_node
from remotemedia.core.data import RuntimeData, DataType, AudioMetadata
import numpy as np

@register_node("audio_processor")
class AudioProcessorNode(MultiprocessNode):
    """Example audio processing node."""

    async def initialize(self):
        """Load models or resources."""
        print(f"Initializing {self.node_id}")
        # Load any ML models, initialize resources
        self.sample_rate = self.config.get("sample_rate", 24000)

    async def process(self, data: RuntimeData) -> RuntimeData:
        """Process audio data."""
        if data.type == DataType.AUDIO:
            # Get audio as numpy array (zero-copy)
            audio = data.as_numpy()

            # Process audio (example: amplify)
            processed = audio * 1.5

            # Return processed data
            return RuntimeData(
                type=DataType.AUDIO,
                payload=processed,
                session_id=data.session_id,
                timestamp=data.timestamp,
                metadata=AudioMetadata(
                    sample_rate=self.sample_rate,
                    channels=1,
                    format="f32"
                )
            )
        return None

    async def cleanup(self):
        """Clean up resources."""
        print(f"Cleaning up {self.node_id}")
```

### 3. Create a Pipeline

```rust
// Create a speech-to-speech pipeline
let pipeline = Pipeline::builder()
    // Whisper ASR node (Python, multiprocess)
    .add_node("whisper", "asr", json!({
        "model": "base",
        "language": "en"
    }))
    .with_executor("python")

    // LFM2 Audio S2S node (Python, multiprocess)
    .add_node("lfm2_audio", "s2s", json!({
        "model": "large",
        "device": "cuda"
    }))
    .with_executor("python")

    // VibeVoice TTS node (Python, multiprocess)
    .add_node("vibe_voice", "tts", json!({
        "voice": "sarah",
        "rate": 1.0
    }))
    .with_executor("python")

    // Connect nodes
    .connect("asr", "text_out", "s2s", "text_in")
    .connect("s2s", "audio_out", "tts", "audio_in")
    .build();

// Create session (spawns all processes)
let session = runtime.create_session("session_123", pipeline).await?;

// Processes are now running concurrently!
```

### 4. Send Data Through Pipeline

```rust
// Send audio to the pipeline
let audio_data = RuntimeData::audio(
    audio_samples, // Vec<f32> or &[f32]
    24000,        // sample rate
    1,            // channels
);

session.send_input("asr", audio_data).await?;

// Receive processed output
let output = session.receive_output("tts").await?;
println!("Processed audio size: {} bytes", output.size());
```

## Python Client Usage

```python
# client.py
import asyncio
from remotemedia.core.multiprocessing import Session, Pipeline

async def main():
    # Create pipeline with multiple Python nodes
    pipeline = (Pipeline()
        .add_node("whisper", "asr", {"model": "base"})
        .add_node("lfm2_audio", "s2s", {"model": "large"})
        .add_node("vibe_voice", "tts", {"voice": "sarah"})
        .connect("asr", "text", "s2s", "input")
        .connect("s2s", "output", "tts", "input"))

    # Create session (spawns processes)
    session = Session("session_123", pipeline)

    # Track initialization progress
    def on_progress(progress):
        print(f"{progress.node_id}: {progress.status} ({progress.progress:.0%})")

    await session.initialize(progress_callback=on_progress)

    # Start processing
    await session.start()

    # Send audio data
    audio = load_audio_file("input.wav")
    await session.send("asr", audio)

    # Get result
    result = await session.receive("tts")
    save_audio_file("output.wav", result)

    # Cleanup (terminates all processes)
    await session.cleanup()

asyncio.run(main())
```

## Monitoring & Debugging

### Check Process Status

```rust
// Get session status
let status = session.get_status().await?;
for (node_id, process_status) in status.nodes {
    println!("{}: {:?}", node_id, process_status);
}

// Monitor resource usage
let metrics = session.get_metrics().await?;
println!("Memory usage: {} MB", metrics.memory_mb);
println!("Active processes: {}", metrics.process_count);
```

### Handle Process Crashes

```rust
// Set up crash handler
runtime.on_process_crash(|event| {
    eprintln!("Process crashed: {} (exit code: {})",
        event.node_id, event.exit_code);

    // Pipeline automatically terminates on crash
    // All other processes cleaned up
});
```

### Debug IPC Channels

```rust
// Check channel statistics
let stats = session.get_channel_stats("asr_to_s2s").await?;
println!("Messages transferred: {}", stats.message_count);
println!("Bytes transferred: {}", stats.bytes_transferred);
println!("Current occupancy: {}/{}", stats.current_occupancy, stats.capacity);
```

## Performance Testing

```bash
# Run latency benchmark
cargo bench --bench multiprocess_latency

# Expected results:
# - Inter-node transfer: <1ms for 10MB buffers
# - Process spawn: ~100ms per node
# - End-to-end S2S: <500ms
```

## Configuration Options

```toml
# runtime.toml
[multiprocess]
max_processes_per_session = 10  # None for unlimited
channel_capacity = 100           # Messages per channel
init_timeout_secs = 30          # Node initialization timeout
cleanup_grace_period_secs = 5   # Graceful shutdown time
enable_backpressure = true      # Block on full channels
shm_segment_size = 268435456    # 256MB shared memory

[multiprocess.python]
executable = "python"            # Python interpreter path
venv_path = ".venv"             # Optional virtualenv
extra_paths = ["./nodes"]       # Additional Python paths
```

## Common Issues

### Issue: Process fails to spawn

```
Error: ProcessSpawnError: Python executable not found
```

**Solution**: Ensure Python is in PATH or specify full path in config.

### Issue: Shared memory exhausted

```
Error: ChannelFullError: Buffer at capacity (backpressure active)
```

**Solution**: Increase `channel_capacity` or optimize processing speed.

### Issue: Initialization timeout

```
Error: InitTimeout: Node 's2s' failed to initialize within 30s
```

**Solution**: Increase `init_timeout_secs` or check node logs for errors.

### Issue: Pipeline terminates unexpectedly

```
Error: ProcessCrashed: Node 'tts' exited with code 137
```

**Solution**: Check for OOM killer (code 137 = SIGKILL). Increase memory limits or optimize model loading.

## Best Practices

1. **Resource Management**
   - Set appropriate process limits based on available memory
   - Pre-load models during initialization, not during processing
   - Monitor memory usage and set limits per node

2. **Error Handling**
   - Always handle `ProcessCrashError` at the application level
   - Implement retry logic at the session level, not node level
   - Log all process exits for debugging

3. **Performance Optimization**
   - Use numpy arrays for audio/video data (zero-copy compatible)
   - Batch small messages to reduce IPC overhead
   - Profile individual nodes before combining in pipeline

4. **Testing**
   - Test each node in isolation first
   - Simulate crashes with `kill -9` to verify cleanup
   - Benchmark with production-size data buffers

## Next Steps

- [API Reference](./contracts/rust-api.md) - Complete Rust API documentation
- [Python API](./contracts/python-api.md) - Python node development guide
- [Architecture](./data-model.md) - Detailed system architecture
- [Examples](../../examples/multiprocess/) - Sample implementations
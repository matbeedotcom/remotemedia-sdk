# Python RuntimeData API Documentation

This document describes the Python RuntimeData API for creating nodes that communicate directly with the Rust runtime.

## Overview

The RuntimeData API provides type-safe, efficient data exchange between Python nodes and the Rust execution engine. Instead of serializing data to JSON, nodes can work directly with native data types like numpy arrays, text, and binary data.

## Benefits

- **Zero-copy for numpy arrays**: Audio and tensor data can be passed without serialization
- **Type-safe**: Compile-time checks ensure correct data types
- **Efficient**: No JSON serialization overhead for large data
- **Streaming-friendly**: Natural support for async generators

## Installation

The RuntimeData bindings are automatically included when you build the RemoteMedia Rust extension:

```bash
cd runtime
cargo build --release
maturin develop  # For development builds
```

## API Reference

### RuntimeData Class

The main class for representing data in the pipeline.

#### Creating RuntimeData

```python
from remotemedia.runtime_data import RuntimeData

# Text data
text_data = RuntimeData.text("Hello, world!")

# Audio data (from raw bytes)
audio_data = RuntimeData.audio(
    samples=audio_bytes,      # bytes: raw audio samples
    sample_rate=24000,        # u32: sample rate in Hz
    channels=1,               # u32: number of channels
    format="f32",            # str: "f32", "i16", or "i32"
    num_samples=24000        # u64: total number of samples
)

# JSON data (from Python dict/list)
json_data = RuntimeData.json({"key": "value", "count": 42})

# Binary data
binary_data = RuntimeData.binary(b"\x00\x01\x02\x03")
```

#### Checking Data Type

```python
# Check the type
data_type = data.data_type()  # Returns: "text", "audio", "json", "binary", etc.

# Type checking
if data.is_text():
    text = data.as_text()
elif data.is_audio():
    audio_info = data.as_audio()
elif data.is_json():
    json_obj = data.as_json()
```

#### Extracting Data

```python
# Extract text
if data.is_text():
    text: str = data.as_text()

# Extract audio
if data.is_audio():
    samples_bytes, sample_rate, channels, format_str, num_samples = data.as_audio()

# Extract JSON
if data.is_json():
    python_obj = data.as_json()  # Returns Python dict/list

# Extract binary
if data.is_binary():
    bytes_obj = data.as_binary()
```

### Numpy Integration

For nodes that process audio or tensor data, use the numpy conversion functions:

```python
from remotemedia.runtime_data import numpy_to_audio, audio_to_numpy
import numpy as np

# Convert numpy array to RuntimeData.Audio
audio_array = np.random.randn(24000).astype(np.float32)
audio_data = numpy_to_audio(
    array=audio_array,
    sample_rate=24000,
    channels=1
)

# Convert RuntimeData.Audio to numpy array
audio_array = audio_to_numpy(audio_data)
# Returns: numpy array with shape (num_samples,) for mono or (num_samples, channels) for multi-channel
```

## Node Implementation Patterns

### Basic Single-Input Node

```python
class MyProcessorNode:
    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.params = params

    async def initialize(self):
        """Called once before processing starts"""
        pass

    async def process(self, data: RuntimeData) -> RuntimeData:
        """Process a single input and return output"""
        # Validate input type
        if not data.is_text():
            raise ValueError(f"Expected text, got {data.data_type()}")

        # Extract and process
        text = data.as_text()
        result = text.upper()

        # Return RuntimeData
        return RuntimeData.text(result)

    async def cleanup(self):
        """Called when processing is done"""
        pass
```

### Streaming Node (Generator)

For nodes that produce multiple outputs (like TTS):

```python
from typing import AsyncGenerator

class StreamingTTSNode:
    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.is_streaming = True  # Important: marks node as streaming

    async def initialize(self):
        # Load TTS model
        pass

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """Process input and yield multiple audio chunks"""
        text = data.as_text()

        # Generate audio chunks
        for chunk in self._synthesize(text):
            audio_data = numpy_to_audio(chunk, sample_rate=24000, channels=1)
            yield audio_data

    async def cleanup(self):
        pass
```

### Audio Processing Node

For nodes that transform audio:

```python
import numpy as np
from remotemedia.runtime_data import audio_to_numpy, numpy_to_audio

class AudioEffectNode:
    def __init__(self, node_id: str, params: dict):
        self.node_id = node_id
        self.gain = params.get('gain', 1.0)

    async def initialize(self):
        pass

    async def process(self, data: RuntimeData) -> RuntimeData:
        """Apply audio effect"""
        # Convert to numpy
        audio_array = audio_to_numpy(data)

        # Process
        processed = audio_array * self.gain
        processed = np.clip(processed, -1.0, 1.0)

        # Get original metadata
        _, sample_rate, channels, _, _ = data.as_audio()

        # Convert back
        return numpy_to_audio(processed, sample_rate, channels)

    async def cleanup(self):
        pass
```

### Multi-Input Node

For nodes that combine multiple inputs (e.g., audio + video):

```python
class SyncNode:
    async def process_multi(self, inputs: dict) -> RuntimeData:
        """
        Process multiple named inputs.

        Args:
            inputs: Dict mapping input names to RuntimeData
                   e.g., {"audio": RuntimeData, "video": RuntimeData}
        """
        audio_data = inputs.get("audio")
        video_data = inputs.get("video")

        # Process both inputs
        # ...

        return result_data
```

## Complete Example: Text-to-Speech Node

```python
"""Complete TTS Node Example"""
import numpy as np
from typing import AsyncGenerator
import asyncio
from remotemedia.runtime_data import RuntimeData, numpy_to_audio

class SimpleTTSNode:
    def __init__(
        self,
        node_id: str,
        voice: str = "default",
        sample_rate: int = 24000
    ):
        self.node_id = node_id
        self.voice = voice
        self.sample_rate = sample_rate
        self.is_streaming = True

        self._tts_engine = None

    async def initialize(self):
        """Initialize TTS engine"""
        # Load TTS model
        from kokoro import KPipeline
        self._tts_engine = await asyncio.to_thread(
            lambda: KPipeline(lang_code='a')
        )

    async def process(self, data: RuntimeData) -> AsyncGenerator[RuntimeData, None]:
        """Synthesize speech from text"""
        # Validate input
        if not data.is_text():
            raise ValueError(f"Expected text input, got {data.data_type()}")

        text = data.as_text()

        # Generate speech in chunks
        generator = await asyncio.to_thread(
            self._create_generator, text
        )

        for graphemes, phonemes, audio in generator:
            # Convert numpy to RuntimeData
            audio_array = np.array(audio, dtype=np.float32)
            audio_data = numpy_to_audio(
                audio_array,
                self.sample_rate,
                channels=1
            )

            yield audio_data

    def _create_generator(self, text: str):
        """Create TTS generator (runs in thread)"""
        return self._tts_engine(
            text,
            voice=self.voice,
            speed=1.0
        )

    async def cleanup(self):
        """Cleanup resources"""
        self._tts_engine = None
```

## Error Handling

Always validate input types and handle errors gracefully:

```python
async def process(self, data: RuntimeData) -> RuntimeData:
    try:
        # Validate type
        if not data.is_text():
            raise ValueError(
                f"{self.node_id} expects text input, got {data.data_type()}"
            )

        # Process
        text = data.as_text()
        result = self._process_text(text)

        return RuntimeData.text(result)

    except Exception as e:
        # Log error with context
        logger.error(f"Node {self.node_id} processing failed: {e}")
        raise
```

## Best Practices

1. **Always validate input types**: Use `is_text()`, `is_audio()`, etc. before extracting
2. **Use numpy for audio**: The `numpy_to_audio` / `audio_to_numpy` functions are optimized
3. **Mark streaming nodes**: Set `self.is_streaming = True` for nodes that yield multiple outputs
4. **Use asyncio for I/O**: Run blocking operations with `asyncio.to_thread()`
5. **Cleanup resources**: Always implement the `cleanup()` method
6. **Type hints**: Use `RuntimeData` and `AsyncGenerator[RuntimeData, None]` for clarity

## Differences from JSON API

### Old JSON-based approach:
```python
async def process(self, data: dict) -> dict:
    text = data.get("text", "")
    # ... process ...
    return {"result": processed}
```

### New RuntimeData approach:
```python
async def process(self, data: RuntimeData) -> RuntimeData:
    text = data.as_text()
    # ... process ...
    return RuntimeData.text(processed)
```

### Key advantages:
- **Type safety**: Compile-time checks prevent type errors
- **Efficiency**: No JSON serialization for audio/video data
- **Clarity**: Explicit data types make code more readable
- **Performance**: Direct memory access for numpy arrays

## Testing

Test your nodes locally before integration:

```python
import asyncio
from remotemedia.runtime_data import RuntimeData

async def test_node():
    node = MyProcessorNode("test_1", {})
    await node.initialize()

    # Test with text input
    input_data = RuntimeData.text("test input")
    output_data = await node.process(input_data)

    assert output_data.is_text()
    assert output_data.as_text() == "TEST INPUT"

    await node.cleanup()

if __name__ == "__main__":
    asyncio.run(test_node())
```

## Integration with Rust Runtime

Your Python nodes automatically work with the Rust streaming pipeline when:

1. They follow the node interface (initialize/process/cleanup)
2. They use RuntimeData for inputs and outputs
3. They're registered in the node registry

The Rust runtime handles:
- Node lifecycle (initialize → process → cleanup)
- Data conversion (proto ↔ RuntimeData ↔ Python)
- Streaming coordination
- Error propagation
- Resource management

## Examples

See the following example files:
- `examples/python_runtime_data_example.py` - Basic examples of all data types
- `examples/audio_examples/kokoro_tts_runtime_data.py` - Complete TTS implementation
- `examples/audio_examples/audio_processor.py` - Audio processing example

## Troubleshooting

### ImportError: cannot import RuntimeData

Make sure the Rust extension is built:
```bash
cd runtime
maturin develop
```

### TypeError: expected RuntimeData, got dict

You're mixing old JSON API with new RuntimeData API. Update your node to use RuntimeData.

### numpy array shape mismatch

Audio arrays should be:
- Mono: shape `(num_samples,)`
- Stereo: shape `(num_samples, 2)`
- Multi-channel: shape `(num_samples, num_channels)`

### Streaming node not yielding

Make sure to:
1. Set `self.is_streaming = True`
2. Use `AsyncGenerator[RuntimeData, None]` return type
3. Use `yield` not `return` for chunks

## Support

For issues or questions:
- Check examples in `examples/` directory
- Review existing nodes in `runtime/src/nodes/`
- File issues at: https://github.com/remotemedia-sdk/issues

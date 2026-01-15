# RemoteMedia Candle Nodes

Native Rust ML inference nodes for RemoteMedia pipelines, powered by [Hugging Face Candle](https://github.com/huggingface/candle).

## Features

- **WhisperNode** - Speech-to-text transcription
- **YoloNode** - Object detection in video frames
- **PhiNode** / **LlamaNode** - LLM text generation

All nodes integrate with RemoteMedia's `StreamingNode` trait for seamless pipeline composition.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
remotemedia-candle-nodes = { path = "../candle-nodes", features = ["whisper"] }
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| `whisper` | Enable Whisper speech-to-text |
| `yolo` | Enable YOLO object detection |
| `llm` | Enable Phi and LLaMA text generation |
| `cuda` | Enable NVIDIA GPU acceleration |
| `metal` | Enable Apple GPU acceleration |
| `all-models` | Enable all model families |

## Quick Start

### Whisper (Speech-to-Text)

```rust
use remotemedia_candle_nodes::{WhisperNode, WhisperConfig};

let config = WhisperConfig {
    model: WhisperModel::Base,
    language: "en".to_string(),
    device: "auto".to_string(),
    ..Default::default()
};

let node = WhisperNode::new("whisper-1", &config)?;
node.initialize().await?;

// Process audio
let result = node.process(audio_data).await?;
```

### YOLO (Object Detection)

```rust
use remotemedia_candle_nodes::{YoloNode, YoloConfig};

let config = YoloConfig {
    model: YoloModel::Yolov8n,
    confidence_threshold: 0.5,
    device: "auto".to_string(),
    ..Default::default()
};

let node = YoloNode::new("yolo-1", &config)?;
let detections = node.process(video_frame).await?;
```

### LLM (Text Generation)

```rust
use remotemedia_candle_nodes::{PhiNode, PhiConfig};

let config = PhiConfig {
    model: PhiModel::Phi2,
    llm: LlmConfig {
        generation: GenerationConfig {
            max_tokens: 256,
            temperature: 0.7,
            ..Default::default()
        },
        ..Default::default()
    },
    ..Default::default()
};

let node = PhiNode::new("phi-1", &config)?;
let response = node.process(RuntimeData::Text(prompt)).await?;
```

## Model Management

Models are automatically downloaded from Hugging Face Hub on first use.

### Cache Location

Models are cached in `~/.cache/huggingface/hub/` (or `$HF_HOME/hub/`).

### CLI Commands

```bash
# List cached models
remotemedia models list

# Download a model for offline use
remotemedia models download openai/whisper-base

# Remove a model
remotemedia models remove openai/whisper-base

# Show cache statistics
remotemedia models stats
```

## Device Selection

Devices are selected automatically with fallback:

1. **CUDA** - If `cuda` feature enabled and NVIDIA GPU available
2. **Metal** - If `metal` feature enabled and on macOS
3. **CPU** - Always available as fallback

Override with the `device` config option:

```rust
config.device = "cpu".to_string();  // Force CPU
config.device = "cuda:0".to_string();  // Specific CUDA device
config.device = "auto".to_string();  // Auto-select (default)
```

## Supported Models

### Whisper
- `tiny` - 39M params, fastest
- `base` - 74M params (default)
- `small` - 244M params
- `medium` - 769M params
- `large-v3` - 1.5B params, most accurate

### YOLO
- `yolov8n` - 3.2M params, fastest (default)
- `yolov8s` - 11.2M params
- `yolov8m` - 25.9M params
- `yolov8l` - 43.7M params
- `yolov8x` - 68.2M params, most accurate

### LLM
- `phi-2` - 2.7B params (default)
- `phi-3-mini` - 3.8B params
- `llama-3.2-1b` - 1B params
- `llama-3.2-3b` - 3B params

## License

Same as RemoteMedia SDK.

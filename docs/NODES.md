# RemoteMedia SDK Node Reference

Complete reference for all built-in nodes in the RemoteMedia SDK. Nodes are the building blocks of pipelines, processing audio, video, and text data.

## Table of Contents

- [Rust Nodes](#rust-nodes)
  - [Audio Processing](#audio-processing)
  - [Low-Latency Streaming](#low-latency-streaming)
  - [Transcription](#transcription)
  - [Health & Monitoring](#health--monitoring)
  - [Utility Nodes](#utility-nodes)
  - [Video Processing](#video-processing)
- [Python Nodes](#python-nodes)
  - [Audio Processing (Python)](#audio-processing-python)
  - [Transcription (Python)](#transcription-python)
  - [Text-to-Speech](#text-to-speech)
  - [I/O Nodes](#io-nodes)
  - [Source & Sink Nodes](#source--sink-nodes)
- [Quick Reference](#quick-reference)

---

## Rust Nodes

High-performance nodes implemented in Rust, offering 2-16x speedup over Python equivalents.

### Audio Processing

#### RustWhisperNode

OpenAI Whisper speech-to-text transcription using native Rust bindings.

```yaml
- id: stt
  node_type: RustWhisperNode
  params:
    model: "base"           # tiny, base, small, medium, large-v2, large-v3-turbo
    language: "en"          # Language code or "auto" for detection
    n_threads: 4            # Inference threads
    translate: false        # Translate to English
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `model` | string | `"base"` | Model size: `tiny`, `tiny.en`, `base`, `base.en`, `small`, `small.en`, `medium`, `medium.en`, `large`, `large-v2`, `large-v3-turbo` |
| `model_path` | string | - | Local path to GGML model file (optional) |
| `language` | string | `"auto"` | Language code (e.g., "en", "fr") or "auto" |
| `n_threads` | int | `4` | Number of inference threads |
| `translate` | bool | `false` | Translate output to English |

**Input:** `Audio` (16kHz, mono, f32)
**Output:** `Json` with transcription result

```json
{
  "text": "Hello world",
  "segments": [{"start": 0.0, "end": 1.5, "text": "Hello world"}],
  "language": "en"
}
```

---

#### FastResampleNode

High-quality audio resampling using Rubato library with zero-copy fast path.

```yaml
- id: resample
  node_type: FastResampleNode
  params:
    target_rate: 16000
    quality: "medium"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `target_rate` | int | `16000` | Target sample rate in Hz (8000-192000) |
| `quality` | string | `"medium"` | Quality level: `low`, `medium`, `high` |

**Quality Settings:**
- `low`: 512 samples/chunk, 64-tap sinc filter
- `medium`: 1024 samples/chunk, 128-tap sinc filter
- `high`: 2048 samples/chunk, 256-tap sinc filter

**Input:** `Audio` (any sample rate)
**Output:** `Audio` (target sample rate)

---

#### SileroVADNode

Voice activity detection using Silero VAD ONNX model.

```yaml
- id: vad
  node_type: SileroVADNode
  params:
    threshold: 0.5
    min_speech_duration_ms: 250
    min_silence_duration_ms: 100
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `threshold` | float | `0.5` | Speech probability threshold (0.0-1.0) |
| `sampling_rate` | int | `16000` | Sample rate (8000 or 16000) |
| `min_speech_duration_ms` | int | `250` | Minimum speech duration to trigger |
| `min_silence_duration_ms` | int | `100` | Minimum silence to end speech |
| `speech_pad_ms` | int | `30` | Padding before/after speech |

**Input:** `Audio`
**Output:** `Json` (VAD events) + `Audio` (passthrough)

```json
{
  "has_speech": true,
  "speech_probability": 0.89,
  "is_speech_start": true,
  "is_speech_end": false
}
```

---

#### AudioChunkerNode

Splits audio into fixed-size chunks for models requiring fixed input sizes.

```yaml
- id: chunker
  node_type: AudioChunkerNode
  params:
    chunk_size: 512
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `chunk_size` | int | `512` | Target chunk size in samples |

**Input:** `Audio`
**Output:** `Audio` (fixed-size chunks)

---

#### AudioBufferAccumulatorNode

Accumulates audio samples into larger buffers before processing.

```yaml
- id: buffer
  node_type: AudioBufferAccumulatorNode
  params:
    target_samples: 16000
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `target_samples` | int | `16000` | Samples to accumulate before output |

**Input:** `Audio`
**Output:** `Audio` (accumulated buffer)

---

### Low-Latency Streaming

These nodes implement **speculative forwarding** for ultra-low-latency voice interaction. Traditional VAD-gated pipelines wait for VAD confirmation before forwarding audio, adding 200-500ms latency. Speculative nodes forward audio immediately and cancel if VAD determines it was a false positive.

**Latency Impact:**
- Traditional VAD-first: 200-500ms added latency (waiting for VAD decision)
- Speculative forwarding: ~0ms added latency (audio forwarded immediately)
- Trade-off: May need to cancel false positives downstream

#### SpeculativeVADGate

Low-level speculative forwarding gate that forwards audio immediately while buffering for potential cancellation.

```yaml
- id: spec_gate
  node_type: SpeculativeVADGate
  params:
    lookback_ms: 150
    lookahead_ms: 50
    vad_threshold: 0.5
    min_speech_ms: 250
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lookback_ms` | int | `150` | Audio buffer size for cancellation (ms) |
| `lookahead_ms` | int | `50` | Wait time before confirming speculation (ms) |
| `sample_rate` | int | `16000` | Audio sample rate in Hz |
| `vad_threshold` | float | `0.5` | Speech probability threshold (0.0-1.0) |
| `min_speech_ms` | int | `250` | Minimum speech duration to accept |
| `min_silence_ms` | int | `100` | Minimum silence to end speech segment |
| `pad_ms` | int | `30` | Padding before/after speech |

**Input:** `Audio`
**Output:** `Audio` (immediate) + `ControlMessage` (CancelSpeculation on false positive)

**Key Behaviors:**
1. **Immediate forwarding**: Audio chunks forwarded without waiting for VAD
2. **Ring buffer storage**: Audio buffered for potential cancellation
3. **Metrics tracking**: Records speculation acceptance rate

**Output Control Messages:**
```json
{
  "message_type": "CancelSpeculation",
  "from_timestamp": 0,
  "to_timestamp": 4000,
  "segment_id": "session_0",
  "metadata": {
    "reason": "vad_false_positive",
    "vad_confidence": 0.3
  }
}
```

---

#### SpeculativeVADCoordinator

All-in-one node that combines speculative forwarding with Silero VAD inference. Preferred for most use cases.

```yaml
- id: spec_vad
  node_type: SpeculativeVADCoordinator
  params:
    vad_threshold: 0.5
    sample_rate: 16000
    min_speech_duration_ms: 250
    min_silence_duration_ms: 100
    lookback_ms: 150
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `vad_threshold` | float | `0.5` | Speech probability threshold (0.0-1.0) |
| `sample_rate` | int | `16000` | Audio sample rate (8000 or 16000) |
| `min_speech_duration_ms` | int | `250` | Minimum speech duration to accept |
| `min_silence_duration_ms` | int | `100` | Minimum silence to end segment |
| `lookback_ms` | int | `150` | Audio buffer for cancellation (ms) |
| `speech_pad_ms` | int | `30` | Padding before/after speech |

**Input:** `Audio`
**Output:** `Audio` (immediate) + `Json` (VAD events) + `ControlMessage` (on false positive)

**How It Works:**
1. **Immediate forwarding**: Audio forwarded without blocking
2. **Parallel VAD**: Silero VAD runs in parallel (doesn't block forwarding)
3. **Speech tracking**: Tracks speech start/end and segment duration
4. **False positive detection**: Emits `CancelSpeculation` if segment < `min_speech_duration_ms`

**Example Pipeline (Voice Assistant with Low Latency):**
```yaml
nodes:
  - id: spec_vad
    node_type: SpeculativeVADCoordinator
    params:
      min_speech_duration_ms: 300
      vad_threshold: 0.6

  - id: stt
    node_type: RustWhisperNode
    params:
      model: "base"

  - id: tts
    node_type: KokoroTTSNode

connections:
  - { from: spec_vad, to: stt }
  - { from: stt, to: tts }
```

**Latency Comparison:**

| Approach | Added Latency | False Positive Handling |
|----------|--------------|------------------------|
| No VAD | 0ms | None (all audio processed) |
| Traditional VAD-first | 200-500ms | Blocked at source |
| **Speculative (this)** | **~0ms** | CancelSpeculation messages |

**When to Use:**
- **Use SpeculativeVADCoordinator**: Most voice assistant use cases
- **Use SpeculativeVADGate**: When you need custom VAD integration or advanced control

**Metrics:**
Both nodes track speculation acceptance rate:
```rust
// Get acceptance rate (0.0-1.0)
let rate = node.get_acceptance_rate("session_id").await;
// 1.0 = all speculations accepted (good VAD)
// 0.5 = half were false positives (noisy environment)
```

---

### Health & Monitoring

#### HealthEmitterNode

Stream health monitoring that emits JSONL events for drift, freezes, and health scores.

```yaml
- id: health
  node_type: HealthEmitterNode
  params:
    freeze_threshold_ms: 500
    health_threshold: 0.7
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lead_threshold_ms` | int | `50` | Drift lead threshold |
| `freeze_threshold_ms` | int | `500` | Freeze detection timeout |
| `av_skew_threshold_ms` | int | `80` | Audio/video sync threshold |
| `health_threshold` | float | `0.7` | Health score threshold |
| `score_change_threshold` | float | `0.05` | Min change to emit health event |

**Input:** `Audio` or `Video`
**Output:** `Json` (JSONL health events)

---

#### AudioLevelNode

Analyzes RMS energy and detects low volume/silence conditions.

```yaml
- id: level
  node_type: AudioLevelNode
  params:
    low_volume_threshold_db: -20
    silence_threshold_db: -60
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `low_volume_threshold_db` | float | `-20` | Low volume threshold in dB |
| `silence_threshold_db` | float | `-60` | Silence threshold in dB |

**Input:** `Audio`
**Output:** `Json`

```json
{
  "rms_db": -15.5,
  "peak_db": -8.2,
  "is_low_volume": false,
  "is_silence": false,
  "health": 1.0
}
```

---

#### SilenceDetectorNode

Detects sustained silence and intermittent audio dropouts.

```yaml
- id: silence
  node_type: SilenceDetectorNode
  params:
    silence_threshold_db: -50
    sustained_silence_ms: 500
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `silence_threshold_db` | float | `-50` | Silence level threshold in dB |
| `sustained_silence_ms` | float | `500` | Duration to trigger sustained alert |
| `dropout_count_threshold` | int | `3` | Silence transitions in window |
| `dropout_window_ms` | float | `5000` | Time window for dropout tracking |

**Input:** `Audio`
**Output:** `Json`

```json
{
  "is_silent": true,
  "silence_duration_ms": 600.0,
  "is_sustained_silence": true,
  "dropout_count": 4,
  "has_intermittent_dropouts": true,
  "health": 0.0
}
```

---

#### ClippingDetectorNode

Detects audio clipping/distortion by analyzing peak saturation.

```yaml
- id: clipping
  node_type: ClippingDetectorNode
  params:
    saturation_threshold: 0.99
    saturation_ratio_threshold: 1.0
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `saturation_threshold` | float | `0.99` | Sample saturation threshold |
| `saturation_ratio_threshold` | float | `1.0` | % of samples for clipping alert |
| `crest_factor_threshold_db` | float | `3.0` | Minimum crest factor in dB |

**Input:** `Audio`
**Output:** `Json`

```json
{
  "saturation_ratio": 2.5,
  "crest_factor_db": 2.1,
  "is_clipping": true,
  "health": 0.5
}
```

---

#### ChannelBalanceNode

Detects stereo channel imbalance and dead channels.

```yaml
- id: balance
  node_type: ChannelBalanceNode
  params:
    imbalance_threshold_db: 10
    dead_channel_threshold_db: -60
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `imbalance_threshold_db` | float | `10` | Imbalance alert level in dB |
| `dead_channel_threshold_db` | float | `-60` | Dead channel silence level in dB |

**Input:** `Audio` (stereo or mono)
**Output:** `Json`

```json
{
  "left_rms_db": -12.0,
  "right_rms_db": -25.0,
  "imbalance_db": 13.0,
  "is_imbalanced": true,
  "has_dead_channel": false,
  "health": 0.3
}
```

---

### Utility Nodes

#### PassThrough

Simple pass-through node that returns input unchanged.

```yaml
- id: pass
  node_type: PassThrough
```

No parameters.

**Input:** Any
**Output:** Same as input

---

#### TextCollectorNode

Accumulates streaming text tokens and yields complete sentences.

```yaml
- id: collector
  node_type: TextCollectorNode
  params:
    min_sentence_length: 3
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `split_pattern` | string | `[.!?;\n]` | Regex for sentence boundaries |
| `min_sentence_length` | int | `3` | Min characters to yield |
| `yield_partial_on_end` | bool | `true` | Yield incomplete on stream end |

**Input:** `Text` (tokens)
**Output:** `Text` (complete sentences)

---

#### RemotePipelineNode

Executes a sub-pipeline on a remote server.

```yaml
- id: remote
  node_type: RemotePipelineNode
  params:
    server_url: "http://localhost:50051"
    pipeline_manifest: "transcribe.yaml"
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `server_url` | string | - | Remote server URL |
| `pipeline_manifest` | string | - | Pipeline manifest to execute |
| `timeout_ms` | int | `30000` | Request timeout |

**Input:** Any
**Output:** Result from remote pipeline

---

#### AudioChannelSplitterNode

Routes audio by speaker to separate streams based on diarization.

```yaml
- id: splitter
  node_type: AudioChannelSplitterNode
  params:
    output_mode: "streams"
    max_speakers: 8
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `output_mode` | string | `"streams"` | `streams` or `channels` |
| `max_speakers` | int | `8` | Maximum speakers to support |
| `stream_id_prefix` | string | `"speaker"` | Prefix for speaker IDs |

**Input:** `Audio` + `Json` (diarization)
**Output:** `Audio` (split by speaker)

---

### Video Processing

#### VideoEncoder

Encodes raw video frames to compressed format.

```yaml
- id: encoder
  node_type: VideoEncoder
  params:
    codec: "h264"
    bitrate: 2000000
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `codec` | string | `"h264"` | Video codec |
| `bitrate` | int | `2000000` | Target bitrate in bps |
| `width` | int | - | Output width |
| `height` | int | - | Output height |

---

#### VideoDecoder

Decodes compressed video to raw frames.

```yaml
- id: decoder
  node_type: VideoDecoder
```

---

#### VideoScaler

Scales video to target resolution.

```yaml
- id: scaler
  node_type: VideoScaler
  params:
    width: 1280
    height: 720
```

---

#### VideoFlip

Flips video horizontally or vertically.

```yaml
- id: flip
  node_type: VideoFlip
  params:
    horizontal: true
    vertical: false
```

---

## Python Nodes

Python nodes for ML models and complex processing, with optional Rust acceleration.

### Audio Processing (Python)

#### AudioResampleNode

Audio resampling with optional Rust acceleration (50-100x faster).

```python
from remotemedia.nodes import AudioResampleNode

node = AudioResampleNode(
    target_sample_rate=16000,
    quality="high",
    runtime_hint="auto"  # "auto", "rust", or "python"
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `target_sample_rate` | int | `16000` | Target sample rate in Hz |
| `quality` | str | `"high"` | `low`, `medium`, `high` |
| `runtime_hint` | str | `"auto"` | Runtime selection |

---

#### VADNode

Voice activity detection with optional Rust acceleration.

```python
from remotemedia.nodes import VADNode

node = VADNode(
    frame_duration_ms=30,
    energy_threshold=0.02,
    runtime_hint="auto"
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `frame_duration_ms` | int | `30` | Frame duration in ms |
| `energy_threshold` | float | `0.02` | Energy threshold for speech |
| `runtime_hint` | str | `"auto"` | Runtime selection |

---

#### VoiceActivityDetector

Streaming VAD with adaptive thresholding and filter mode.

```python
from remotemedia.nodes import VoiceActivityDetector

node = VoiceActivityDetector(
    frame_duration_ms=30,
    energy_threshold=0.02,
    speech_threshold=0.3,
    filter_mode=False
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `frame_duration_ms` | int | `30` | Frame duration (10, 20, or 30) |
| `energy_threshold` | float | `0.02` | Energy threshold (0.0-1.0) |
| `speech_threshold` | float | `0.3` | Ratio of speech frames |
| `filter_mode` | bool | `False` | Only output speech segments |

---

#### FormatConverterNode

Converts audio between sample formats (f32, i16, i32).

```python
from remotemedia.nodes import FormatConverterNode

node = FormatConverterNode(
    target_format="f32",  # "f32", "i16", "i32"
    runtime_hint="auto"
)
```

---

### Transcription (Python)

#### WhisperXTranscriber

WhisperX transcription with VAD preprocessing and word-level timestamps.

```python
from remotemedia.nodes import WhisperXTranscriber

node = WhisperXTranscriber(
    model_size="base",
    device="cuda",
    compute_type="float16",
    language="en",
    align_model=True
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `model_size` | str | `"base"` | `tiny`, `base`, `small`, `medium`, `large-v2`, `large-v3` |
| `device` | str | `"cpu"` | `cpu` or `cuda` |
| `compute_type` | str | `"float32"` | `float16`, `int8`, `float32` |
| `batch_size` | int | `16` | Inference batch size |
| `language` | str | `None` | Language code or None for auto-detect |
| `align_model` | bool | `False` | Enable word-level timestamps |
| `vad_onset` | float | `0.5` | VAD onset threshold |
| `vad_offset` | float | `0.363` | VAD offset threshold |

---

#### RustWhisperTranscriber

Pass-through node executed by Rust runtime for maximum performance.

```python
from remotemedia.nodes import RustWhisperTranscriber

node = RustWhisperTranscriber(
    model_source="base",
    language="en",
    n_threads=4
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `model_path` | str | `None` | Path to GGML model file |
| `model_source` | str | `None` | Pre-defined model name |
| `language` | str | `None` | Language code |
| `n_threads` | int | `4` | Inference threads |

---

### Text-to-Speech

#### KokoroTTSNode

Text-to-speech using Kokoro TTS (82M parameter model).

```python
from remotemedia.nodes import KokoroTTSNode

node = KokoroTTSNode(
    lang_code='a',      # 'a'=American, 'b'=British, 'j'=Japanese, etc.
    voice='af_heart',
    speed=1.0,
    sample_rate=24000
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `lang_code` | str | `'a'` | Language: `a`=American, `b`=British, `e`=Spanish, `f`=French, `h`=Hindi, `i`=Italian, `j`=Japanese, `p`=Portuguese, `z`=Chinese |
| `voice` | str | `'af_heart'` | Voice identifier |
| `speed` | float | `1.0` | Speech speed multiplier |
| `sample_rate` | int | `24000` | Output sample rate |
| `stream_chunks` | bool | `True` | Enable audio chunk streaming |

**Input:** `Text`
**Output:** `Audio` (mono, f32 @ sample_rate)

---

#### VibeVoiceTTSNode

Text-to-speech with voice cloning support.

```python
from remotemedia.nodes import VibeVoiceTTSNode

node = VibeVoiceTTSNode(
    model_path="/path/to/model",
    device="cuda",
    use_voice_cloning=True,
    voice_samples=["reference.wav"]
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `model_path` | str | `"/tmp/vibevoice-model"` | Path to model files |
| `device` | str | `None` | `cuda`, `mps`, `cpu`, or auto |
| `inference_steps` | int | `10` | Denoising steps |
| `cfg_scale` | float | `1.3` | Classifier-free guidance |
| `sample_rate` | int | `24000` | Output sample rate |
| `use_voice_cloning` | bool | `False` | Enable voice cloning |
| `voice_samples` | list | `None` | Voice sample file paths |

---

### I/O Nodes

#### DataSourceNode

Source node that receives data from external systems.

```python
from remotemedia.nodes import DataSourceNode

node = DataSourceNode(
    buffer_size=100,
    timeout_seconds=30.0
)

# Push data from external system
await node.push_data(data)
```

---

#### DataSinkNode

Sink node that sends processed data to external systems.

```python
from remotemedia.nodes import DataSinkNode

def my_callback(data):
    print(f"Received: {data}")

node = DataSinkNode(
    callback=my_callback,
    buffer_output=True
)
```

---

#### JavaScriptBridgeNode

Integration between JavaScript clients and Python pipelines.

```python
from remotemedia.nodes import JavaScriptBridgeNode

node = JavaScriptBridgeNode(
    transform_input=lambda x: x,  # JS → Python
    transform_output=lambda x: x  # Python → JS
)
```

---

### Source & Sink Nodes

#### MediaReaderNode

Reads media frames from local files or URLs.

```python
from remotemedia.nodes import MediaReaderNode

node = MediaReaderNode(path="input.mp4")
```

---

#### MediaWriterNode

Writes media frames to local files.

```python
from remotemedia.nodes import MediaWriterNode

node = MediaWriterNode(output_path="output.mp4")
```

---

## Quick Reference

### All Node Types

| Node Type | Runtime | Category | Input | Output |
|-----------|---------|----------|-------|--------|
| `RustWhisperNode` | Rust | Transcription | Audio | Json |
| `FastResampleNode` | Rust | Audio | Audio | Audio |
| `SileroVADNode` | Rust | Audio | Audio | Json+Audio |
| `AudioChunkerNode` | Rust | Audio | Audio | Audio |
| `AudioBufferAccumulatorNode` | Rust | Audio | Audio | Audio |
| `HealthEmitterNode` | Rust | Monitoring | Audio/Video | Json |
| `AudioLevelNode` | Rust | Monitoring | Audio | Json |
| `SilenceDetectorNode` | Rust | Monitoring | Audio | Json |
| `ClippingDetectorNode` | Rust | Monitoring | Audio | Json |
| `ChannelBalanceNode` | Rust | Monitoring | Audio | Json |
| `PassThrough` | Rust | Utility | Any | Any |
| `TextCollectorNode` | Rust | Utility | Text | Text |
| `RemotePipelineNode` | Rust | Routing | Any | Any |
| `AudioChannelSplitterNode` | Rust | Audio | Audio+Json | Audio |
| `VideoEncoder` | Rust | Video | Video | Video |
| `VideoDecoder` | Rust | Video | Video | Video |
| `VideoScaler` | Rust | Video | Video | Video |
| `VideoFlip` | Rust | Video | Video | Video |
| `KokoroTTSNode` | Python | TTS | Text | Audio |
| `VibeVoiceTTSNode` | Python | TTS | Text | Audio |
| `WhisperXTranscriber` | Python | Transcription | Audio | Json |
| `HFWhisperNode` | Python | Transcription | Audio | Json |
| `MediaReaderNode` | Python | I/O | - | Audio/Video |
| `MediaWriterNode` | Python | I/O | Audio/Video | - |

### Common Pipeline Patterns

**Transcription Pipeline:**
```yaml
nodes:
  - id: resample
    node_type: FastResampleNode
    params: { target_rate: 16000 }
  - id: vad
    node_type: SileroVADNode
  - id: stt
    node_type: RustWhisperNode
    params: { model: "base" }
connections:
  - { from: resample, to: vad }
  - { from: vad, to: stt }
```

**Voice Assistant Pipeline:**
```yaml
nodes:
  - id: vad
    node_type: SileroVADNode
  - id: stt
    node_type: RustWhisperNode
  - id: tts
    node_type: KokoroTTSNode
connections:
  - { from: vad, to: stt }
  - { from: stt, to: tts }
```

**Stream Health Monitoring:**
```yaml
nodes:
  - id: level
    node_type: AudioLevelNode
  - id: silence
    node_type: SilenceDetectorNode
  - id: clipping
    node_type: ClippingDetectorNode
  - id: health
    node_type: HealthEmitterNode
connections:
  - { from: level, to: health }
  - { from: silence, to: health }
  - { from: clipping, to: health }
```

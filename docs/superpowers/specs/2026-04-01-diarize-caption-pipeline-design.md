# Diarization + STT Live Caption Pipeline

**Date**: 2026-04-01
**Status**: Approved
**Purpose**: Real-time speaker-attributed live captions for game audio

## Problem

Game audio captured by an APO needs real-time speaker diarization and transcription to produce live captions with speaker attribution. The current `SpeakerDiarizationNode` emits speaker segments as a separate JSON output alongside passthrough audio, meaning speaker identity is lost by the time audio reaches the STT node. There's no way to annotate audio with metadata that flows through the pipeline.

## Solution

Three changes:

1. Add a `metadata` field to `RuntimeData::Audio` for pipeline-flowing annotations
2. Fix `SpeakerDiarizationNode` to embed speaker info into audio metadata instead of separate outputs
3. Modify `candle-whisper` to forward input metadata into its text/JSON output

Plus a new production pipeline and self-contained test script.

## Design

### 1. Core Change: `metadata` on `RuntimeData::Audio`

Add an optional extensible metadata field to the `Audio` variant in `crates/core/src/lib.rs`:

```rust
Audio {
    samples: Vec<f32>,
    sample_rate: u32,
    channels: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stream_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    arrival_ts_us: Option<u64>,
    /// Optional extensible metadata (e.g., speaker diarization, confidence scores)
    /// Flows through pipeline with the audio data. Nodes that don't use it ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
},
```

**Backward compatibility**: `#[serde(default)]` means existing serialized data without `metadata` deserializes as `None`. `skip_serializing_if` means it's omitted when `None`. All existing construction sites need `metadata: None` added.

**Impact**: ~158 construction sites across the repo. All get `metadata: None` — a mechanical change with no behavioral impact on existing code.

**Metadata propagation convention**: Nodes that transform audio (resample, chunk, split) SHOULD propagate `metadata` from input to output. Nodes that generate new audio (TTS, oscillators) set `metadata: None`. This is a convention, not enforced by the type system.

**Protobuf change**: Add `optional bytes metadata_json = 6` to the `AudioBuffer` message in `proto/common.proto`. Serialized as JSON bytes (UTF-8). The gRPC adapter (`crates/transports/grpc/src/adapters.rs`) and WebRTC adapter (`crates/transports/webrtc/src/adapters.rs`) must serialize `metadata` as JSON bytes when `Some`, and deserialize back to `serde_json::Value` when present. Using `optional bytes` (not `optional string`) to allow future binary metadata formats. Field number 6 is next available.

**IPC change**: The iceoryx2 IPC binary format in `crates/core/src/python/multiprocess/data_transfer.rs` currently stores audio as raw f32 bytes with no header (sample_rate and channels are already hardcoded TODOs at lines 894-895 of `multiprocess_executor.rs`). The audio payload format must be extended to include metadata:

```
Audio IPC payload (new format):
  sample_rate (4 bytes, u32 LE)
  channels    (2 bytes, u16 LE)
  metadata_len (4 bytes, u32 LE, 0 = no metadata)
  metadata     (metadata_len bytes, UTF-8 JSON, omitted if len=0)
  samples      (remaining bytes, f32 LE)
```

This also fixes the existing TODO for sample_rate/channels. The changes are in:
- `data_transfer.rs`: `RuntimeData::audio()` constructor — add header fields
- `multiprocess_executor.rs`: `core_to_ipc()` (line 797) — serialize metadata JSON
- `multiprocess_executor.rs`: `DataType::Audio` deserialization (line 883) — parse header
- Python side `node.py`: `_receive_input()` — parse the new header format

The Python deserializer in `clients/python/remotemedia/core/multiprocessing/node.py` must be updated to parse the new audio header. Backward compatibility: if metadata_len is 0, no metadata bytes follow — same as before but with the sample_rate/channels header added.

**PartialEq impact**: `RuntimeData` derives `PartialEq`. `serde_json::Value` implements `PartialEq`, so compilation is fine. Tests that compare `Audio` values will need `metadata` in their expected values.

### 2. SpeakerDiarizationNode Fix

**File**: `crates/core/src/nodes/speaker_diarization.rs`

**Current behavior** (lines 353-369): Emits two outputs via callback:
1. `RuntimeData::Json` with `{segments, num_speakers, time_offset, duration}`
2. `RuntimeData::Audio` (original, unchanged) if `passthrough_audio: true`

**New behavior**: Emits a single `RuntimeData::Audio` output with speaker segments embedded in `metadata`:

```rust
// Extract original audio fields
let RuntimeData::Audio { samples, sample_rate, channels, stream_id, timestamp_us, arrival_ts_us, .. } = data else { ... };

// Emit single annotated audio
callback(RuntimeData::Audio {
    samples,
    sample_rate,
    channels,
    stream_id,
    timestamp_us,
    arrival_ts_us,
    metadata: Some(serde_json::json!({
        "diarization": {
            "segments": speaker_segments,  // [{start, end, speaker}, ...]
            "num_speakers": num_speakers,
            "time_offset": time_offset,
            "duration": mono.len() as f64 / self.sample_rate as f64,
        }
    })),
})?;
```

The `passthrough_audio` config option is retained — when `false`, no output is emitted (sink behavior). When `true` (default), the annotated audio is emitted.

**Schema update**: The node schema at line 442 currently declares `produces([RuntimeDataType::Json, RuntimeDataType::Audio])`. This must change to `produces([RuntimeDataType::Audio])` since the node no longer emits a separate JSON output. Any existing pipeline relying on the JSON output from this node will need updating — this is a breaking change to the node's output contract, but the old behavior was not useful in sequential pipelines (the JSON had no downstream consumer).

### 3. candle-whisper Metadata Forwarding

**File**: `crates/candle-nodes/src/whisper/mod.rs`

**Current behavior** (line 331-342): The `process()` method calls `RuntimeDataConverter::extract_audio(&data, ...)` which returns an `AudioData` struct containing only `samples`, `sample_rate`, `channels` (defined in `crates/candle-nodes/src/convert.rs:98-104`). The original `RuntimeData` (including metadata) is consumed. Returns `RuntimeData::Text(transcription)`.

**New behavior**: Extract metadata from the input `RuntimeData` BEFORE calling `extract_audio`, then forward it into the output:

```rust
async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
    // Extract metadata before consuming the RuntimeData
    let input_metadata = match &data {
        RuntimeData::Audio { metadata, .. } => metadata.clone(),
        _ => None,
    };

    // Extract audio (consumes data)
    let audio = RuntimeDataConverter::extract_audio(&data, &self.node_id)
        .map_err(|e| Error::Execution(e.to_string()))?;

    // Transcribe
    let transcription = self.transcribe(audio).await
        .map_err(|e| Error::Execution(e.to_string()))?;

    // Forward metadata if present
    match input_metadata {
        Some(meta) if meta.get("diarization").is_some() => {
            Ok(RuntimeData::Json(serde_json::json!({
                "text": transcription,
                "diarization": meta["diarization"],
            })))
        }
        _ => Ok(RuntimeData::Text(transcription)),
    }
}
```

**Backward compatibility note**: Without diarization metadata, whisper still returns `RuntimeData::Text`. When metadata IS present, it returns `RuntimeData::Json`. Downstream consumers that pattern-match on `Text` specifically will not see the `Json` variant — pipeline authors should use the diarize-caption pipeline with a JSON-aware output sink. This is documented behavior, not a silent change.

### 4. Production Pipeline: `diarize-caption.yaml`

**File**: `examples/cli/pipelines/diarize-caption.yaml`

```yaml
version: v1
metadata:
  name: diarize-caption
  description: Real-time speaker-attributed live captions for game audio

nodes:
  - id: resample
    node_type: FastResampleNode
    params:
      target_rate: 16000
      quality: Medium

  - id: diarize
    node_type: SpeakerDiarizationNode
    params:
      search_threshold: 0.5
      sample_rate: 16000
      passthrough_audio: true
      max_speakers: 10

  - id: transcribe
    node_type: candle-whisper
    params:
      model_source: base.en
      language: en

connections:
  - from: resample
    to: diarize
  - from: diarize
    to: transcribe
```

**Data flow**:
```
stdin/pipe (APO audio) → FastResampleNode (→16kHz) → SpeakerDiarizationNode
  (annotates audio.metadata with speaker segments) → candle-whisper
  (transcribes, merges speaker info) → JSON output
```

**Output format** (per chunk):
```json
{
  "text": "Watch out behind you!",
  "diarization": {
    "segments": [
      {"start": 0.0, "end": 1.52, "speaker": "0"}
    ],
    "num_speakers": 1,
    "time_offset": 0.0,
    "duration": 1.52
  }
}
```

**CLI usage** (production):
```bash
# APO audio piped in, JSON stream out
audio_capture | remotemedia stream diarize-caption.yaml -i - -O -
```

### 5. TTS Voice B Pipeline: `tts-voice-b.yaml`

**File**: `examples/cli/pipelines/tts-voice-b.yaml`

Identical to `tts.yaml` but uses a male voice (`am_adam`) for generating distinct-speaker test audio.

```yaml
version: v1
metadata:
  name: tts-voice-b
  description: TTS with male voice for testing speaker diarization

nodes:
  - id: input
    node_type: TextInput
    params:
      encoding: utf-8

  - id: kokoro
    node_type: KokoroTTSNode
    params:
      voice: am_adam
      speed: 1.0
      sample_rate: 24000
    python_deps:
      - "kokoro>=0.9.4"
      - soundfile

  - id: output
    node_type: AudioOutput
    params:
      sample_rate: 24000
      channels: 1
      format: f32

connections:
  - from: input
    to: kokoro
  - from: kokoro
    to: output
```

### 6. Test Script: `test-diarize-caption.sh`

**File**: `examples/cli/tests/test-diarize-caption.sh`

Self-contained end-to-end test:

1. **Generate test audio** using KokoroTTS (two voices):
   - Voice A (`af_bella`): "Watch out there's an enemy behind you"
   - Voice B (`am_adam`): "I need healing can someone help me"

2. **Concatenate** the two PCM files with a brief silence gap between them.

3. **Run** the diarization+caption pipeline on the combined audio.

4. **Validate** output:
   - JSON output contains `text` and `diarization` fields
   - At least 2 distinct speaker IDs in the output
   - Transcribed text contains keywords from both input phrases

**Prerequisites**: CLI built with `--features candle,speaker-diarization`. `PYTHON_ENV_MODE=managed` for KokoroTTS.

**Model path resolution**: The `SpeakerDiarizationNode` reads model paths via `option_env!("SPEAKER_DIARIZATION_MODELS_DIR")` which is a **compile-time** env var set by `crates/core/build.rs:128`. The build script auto-downloads models to a platform-specific cache dir during compilation when the `speaker-diarization` feature is enabled. The test script does NOT need to download models separately — they are baked into the binary at build time. The test script's model download step is removed; instead, it relies on the build having the models available.

**Output format**: The pipeline produces a single JSON object for the entire input (unary mode). The test uses `json.load()` which handles this correctly. For streaming mode (`remotemedia stream`), each chunk produces a separate JSON object (JSONL). The test uses `run` (unary), not `stream`.

```bash
#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CLI="${SCRIPT_DIR}/../../target/debug/remotemedia"
PIPELINES="${SCRIPT_DIR}/../pipelines"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

# Diarization models are compiled into the binary by build.rs
# when --features speaker-diarization is enabled. No runtime download needed.

# Step 1: Generate test audio (two different voices)
echo "=== Step 1: Generating test audio ==="
echo "Generating voice A (female)..."
echo "Watch out there's an enemy behind you" | \
    PYTHON_ENV_MODE=managed $CLI run "$PIPELINES/tts.yaml" -i - -O "$TMP_DIR/voice_a.pcm" --timeout 120

echo "Generating voice B (male)..."
echo "I need healing can someone help me" | \
    PYTHON_ENV_MODE=managed $CLI run "$PIPELINES/tts-voice-b.yaml" -i - -O "$TMP_DIR/voice_b.pcm" --timeout 120

# Step 2: Concatenate with silence gap (0.5s at 24kHz = 12000 zero samples = 48000 bytes)
python3 -c "import sys; sys.stdout.buffer.write(b'\x00' * 48000)" > "$TMP_DIR/silence.pcm"
cat "$TMP_DIR/voice_a.pcm" "$TMP_DIR/silence.pcm" "$TMP_DIR/voice_b.pcm" > "$TMP_DIR/two_speakers.pcm"

echo "Combined audio: $(wc -c < "$TMP_DIR/two_speakers.pcm") bytes"

# Step 3: Run diarize+caption pipeline
echo "Running diarization + transcription..."
$CLI run "$PIPELINES/diarize-caption.yaml" \
    -i "$TMP_DIR/two_speakers.pcm" \
    --input-format raw-pcm \
    -O "$TMP_DIR/output.json" \
    -o json \
    --timeout 120 2>&1 | tee "$TMP_DIR/run.log"

echo "=== Pipeline Output ==="
cat "$TMP_DIR/output.json"

# Step 4: Validate output
echo ""
echo "=== Validation ==="

# Check JSON structure
if python3 -c "
import json, sys
data = json.load(open('$TMP_DIR/output.json'))
assert 'text' in data, 'Missing text field'
assert 'diarization' in data, 'Missing diarization field'
segments = data['diarization']['segments']
speakers = set(s['speaker'] for s in segments)
print(f'Speakers found: {speakers}')
print(f'Text: {data[\"text\"]}')
assert len(speakers) >= 2, f'Expected >= 2 speakers, got {len(speakers)}'
text_lower = data['text'].lower()
assert any(w in text_lower for w in ['watch', 'enemy', 'behind']), 'Missing keywords from voice A'
assert any(w in text_lower for w in ['healing', 'help']), 'Missing keywords from voice B'
print('✓ All validations passed')
"; then
    echo "TEST PASSED"
    exit 0
else
    echo "TEST FAILED"
    exit 1
fi
```

## Files to Create/Modify

| File | Action | Complexity |
|---|---|---|
| `crates/core/src/lib.rs` | Add `metadata` field to `Audio` variant | Low |
| ~158 files across repo | Add `metadata: None` to all `Audio` constructions | Mechanical |
| `proto/common.proto` | Add `optional bytes metadata_json = 6` to `AudioBuffer` | Low |
| `crates/transports/grpc/src/adapters.rs` | Serialize/deserialize metadata in both directions | Medium |
| `crates/transports/webrtc/src/adapters.rs` | Serialize/deserialize metadata in both directions | Medium |
| `crates/core/src/python/multiprocess/data_transfer.rs` | Extend audio payload with header (sample_rate, channels, metadata) | Medium |
| `crates/core/src/python/multiprocess/multiprocess_executor.rs` | Serialize/deserialize metadata in IPC conversions | Medium |
| `clients/python/remotemedia/core/multiprocessing/node.py` | Parse new audio IPC header format | Medium |
| `crates/core/src/nodes/speaker_diarization.rs` | Emit single annotated audio output | Medium |
| `crates/candle-nodes/src/whisper/mod.rs` | Extract metadata before `extract_audio`, forward into output | Medium |
| `examples/cli/pipelines/diarize-caption.yaml` | New production pipeline | New file |
| `examples/cli/pipelines/tts-voice-b.yaml` | TTS with male voice | New file |
| `examples/cli/tests/test-diarize-caption.sh` | Self-contained test script | New file |

## Feature Requirements

CLI must be built with:
```bash
cargo build --features candle,speaker-diarization
```

Test requires `PYTHON_ENV_MODE=managed` for KokoroTTS dependency installation.

## Risk Assessment

- **158 construction sites**: Mechanical change, risk is typos. Mitigated by `cargo build` catching missing fields.
- **Diarization accuracy**: Two distinct TTS voices should produce clearly separable embeddings. If diarization doesn't detect 2 speakers, the `search_threshold` param can be tuned.
- **candle-whisper output type change**: When metadata is present, output changes from `Text` to `Json`. Consumers pattern-matching on `Text` specifically won't see the `Json` variant. This is intentional — the diarize-caption pipeline is designed for JSON-aware sinks. Without metadata, whisper still returns `Text` as before.
- **Diarization node schema change**: `produces` changes from `[Json, Audio]` to `[Audio]`. Any existing pipeline consuming the JSON output from this node will break. This is acceptable — the old JSON output had no practical consumer in sequential pipelines.
- **IPC format change**: Breaking change to the iceoryx2 audio binary format. Old Python nodes will fail to parse the new header. Both Rust and Python sides must be updated together. Mitigated by the fact that the IPC format is internal (not a public API) and both sides are always deployed together.
- **Proto regeneration**: Adding a field to `AudioBuffer` requires regenerating protobuf code via `prost`. Existing clients that don't send the field will deserialize it as `None` (proto3 optional semantics).

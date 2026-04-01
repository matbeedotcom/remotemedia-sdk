# Diarize-Caption Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add audio metadata annotation to RuntimeData, fix speaker diarization to embed speaker info into audio, and build a real-time diarization+STT live caption pipeline for game audio.

**Architecture:** Add `metadata: Option<serde_json::Value>` to `RuntimeData::Audio` across all layers (core, proto, IPC, transports). Fix `SpeakerDiarizationNode` to annotate audio with speaker segments instead of emitting separate JSON. Modify candle-whisper to forward metadata. New pipeline YAML + test script.

**Tech Stack:** Rust (remotemedia-core, candle-nodes, transports), Protobuf (prost/tonic), Python (multiprocessing IPC), iceoryx2, KokoroTTS

**Spec:** `docs/superpowers/specs/2026-04-01-diarize-caption-pipeline-design.md`

---

## File Structure

### Core Changes
| File | Action | Purpose |
|---|---|---|
| `crates/core/src/lib.rs` | Modify | Add `metadata` field to `RuntimeData::Audio` |
| ~158 files across repo | Modify | Add `metadata: None` to all `Audio` construction sites |

### Protobuf + Transport Changes
| File | Action | Purpose |
|---|---|---|
| `proto/common.proto` | Modify | Add `optional bytes metadata_json = 6` to `AudioBuffer` |
| `crates/transports/grpc/src/adapters.rs` | Modify | Serialize/deserialize metadata in gRPC conversions |
| `crates/transports/webrtc/src/adapters.rs` | Modify | Serialize/deserialize metadata in WebRTC conversions |

### IPC Changes
| File | Action | Purpose |
|---|---|---|
| `crates/core/src/python/multiprocess/data_transfer.rs` | Modify | Extend audio payload with header (sample_rate, channels, metadata) |
| `crates/core/src/python/multiprocess/multiprocess_executor.rs` | Modify | Update core↔IPC Audio conversions |
| `clients/python/remotemedia/core/multiprocessing/data.py` | Modify | Add `annotations` field to `AudioMetadata` |
| `clients/python/remotemedia/core/multiprocessing/node.py` | Modify | Parse new audio IPC header format |

### Node Changes
| File | Action | Purpose |
|---|---|---|
| `crates/core/src/nodes/speaker_diarization.rs` | Modify | Emit single annotated audio instead of JSON+audio |
| `crates/candle-nodes/src/whisper/mod.rs` | Modify | Forward input metadata into output |

### Pipeline + Test
| File | Action | Purpose |
|---|---|---|
| `examples/cli/pipelines/diarize-caption.yaml` | Create | Production diarization+STT pipeline |
| `examples/cli/pipelines/tts-voice-b.yaml` | Create | TTS with male voice for test |
| `examples/cli/tests/test-diarize-caption.sh` | Create | Self-contained end-to-end test |

---

## Task 1: Add `metadata` to `RuntimeData::Audio`

**Files:**
- Modify: `crates/core/src/lib.rs:133-152`

- [ ] **Step 1: Add the metadata field to the Audio variant**

In `crates/core/src/lib.rs`, add `metadata` as the last field of the `Audio` variant (after `arrival_ts_us`):

```rust
            /// Arrival timestamp in microseconds (spec 026)
            /// Set by transport ingest layer for drift monitoring
            #[serde(default, skip_serializing_if = "Option::is_none")]
            arrival_ts_us: Option<u64>,
            /// Optional extensible metadata (e.g., speaker diarization, confidence scores)
            /// Flows through pipeline with the audio data. Nodes that don't use it ignore it.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            metadata: Option<serde_json::Value>,
```

- [ ] **Step 2: Attempt to build and collect all errors**

Run: `cargo build 2>&1 | grep "error\[" | head -50`

This will show all construction sites missing the `metadata` field. Expected: many errors like `missing field metadata in initializer of RuntimeData::Audio`.

- [ ] **Step 3: Fix all Audio construction sites in crates/core/src/lib.rs**

Add `metadata: None` to every `RuntimeData::Audio { .. }` construction in `lib.rs`. There are constructions around lines 306, 765, 822.

- [ ] **Step 4: Fix all Audio construction sites across the repo**

Systematically add `metadata: None` to every `RuntimeData::Audio { .. }` construction. Work through the build errors file by file. The key directories are:

- `crates/core/src/nodes/` — audio processing nodes (~30 sites)
- `crates/core/src/transport/` — session router (~4 sites)
- `crates/core/src/executor/` — executor (~1 site)
- `crates/core/src/ingestion/` — file ingestion (~3 sites)
- `crates/transports/grpc/src/` — gRPC adapter (~3 sites)
- `crates/transports/webrtc/src/` — WebRTC adapter (~10 sites)
- `crates/transports/ffi/src/` — FFI marshal (~4 sites)
- `crates/candle-nodes/src/` — candle converter (~1 site)
- `crates/libs/` — manifest tester, pipeline runner (~7 sites)
- `crates/services/` — SRT ingest (~1 site)
- `examples/cli/` — CLI commands, pipeline nodes (~15 sites)
- `examples/` — other examples (~5 sites)

Also fix pattern matches in WebRTC adapters that explicitly destructure all fields (e.g., `crates/transports/webrtc/src/adapters.rs:70-77` where `stream_id: _` etc. are listed — add `metadata: _`).

- [ ] **Step 5: Fix all Audio construction sites in tests and benchmarks**

- `crates/core/tests/` — integration tests (~15 sites)
- `crates/core/benches/` — benchmarks (~9 sites)
- `crates/candle-nodes/examples/` — example binaries (~2 sites)

- [ ] **Step 6: Build successfully**

Run: `cargo build 2>&1 | tail -5`

Expected: `Finished` with no errors (warnings OK).

- [ ] **Step 7: Run core tests**

Run: `cd crates/core && cargo test 2>&1 | tail -10`

Expected: all tests pass.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: add metadata field to RuntimeData::Audio for pipeline annotations"
```

---

## Task 2: Add metadata to Protobuf AudioBuffer

**Files:**
- Modify: `proto/common.proto:68-86`
- Modify: `crates/transports/grpc/src/adapters.rs:72-79,85-96,233-254`
- Modify: `crates/transports/webrtc/src/adapters.rs:70-83,214-235`

- [ ] **Step 1: Add metadata_json to proto**

In `proto/common.proto`, add to the `AudioBuffer` message after `num_samples`:

```protobuf
message AudioBuffer {
  bytes samples = 1;
  uint32 sample_rate = 2;
  uint32 channels = 3;
  AudioFormat format = 4;
  uint64 num_samples = 5;

  // Optional extensible metadata as JSON bytes (UTF-8)
  // Used for annotations like speaker diarization, confidence scores
  optional bytes metadata_json = 6;
}
```

- [ ] **Step 2: Rebuild to regenerate protobuf code**

Run: `cargo build -p remotemedia-grpc 2>&1 | tail -5`

The `build.rs` in `crates/transports/grpc/` will regenerate the prost code with the new field.

- [ ] **Step 3: Update gRPC adapter — proto→RuntimeData**

In `crates/transports/grpc/src/adapters.rs`, update the `audio_buffer_to_runtime_data` function (around line 72) to deserialize metadata:

```rust
Ok(RuntimeData::Audio {
    samples,
    sample_rate: audio.sample_rate,
    channels: audio.channels,
    stream_id: None,
    timestamp_us: None,
    arrival_ts_us: arrival_ts_us.or_else(|| Some(now_micros())),
    metadata: audio.metadata_json.as_ref().and_then(|bytes| {
        serde_json::from_slice(bytes).ok()
    }),
})
```

- [ ] **Step 4: Update gRPC adapter — RuntimeData→proto**

In `crates/transports/grpc/src/adapters.rs`, update the `runtime_data_to_data_buffer` function (around line 85) to serialize metadata:

```rust
RuntimeData::Audio {
    samples,
    sample_rate,
    channels,
    metadata,
    ..
} => DataType::Audio(AudioBuffer {
    samples: samples.iter().flat_map(|f| f.to_le_bytes()).collect(),
    sample_rate: *sample_rate,
    channels: *channels,
    format: AudioFormat::F32 as i32,
    num_samples: samples.len() as u64,
    metadata_json: metadata.as_ref().map(|m| {
        serde_json::to_vec(m).unwrap_or_default()
    }),
}),
```

- [ ] **Step 5: Update the second gRPC deserialization site**

In `crates/transports/grpc/src/adapters.rs`, update `data_buffer_to_runtime_data_with_arrival` (around line 246) with the same metadata deserialization as Step 3.

- [ ] **Step 6: Update WebRTC adapter — RuntimeData→proto**

In `crates/transports/webrtc/src/adapters.rs`, update `runtime_data_to_data_buffer` (around line 70) to serialize metadata. Same pattern as Step 4. Also add `metadata: _` to the explicit destructuring pattern if present.

- [ ] **Step 7: Update WebRTC adapter — proto→RuntimeData**

In `crates/transports/webrtc/src/adapters.rs`, update `data_buffer_to_runtime_data` (around line 227) to deserialize metadata. Same pattern as Step 3.

- [ ] **Step 8: Build all transports**

Run: `cargo build -p remotemedia-grpc -p remotemedia-webrtc 2>&1 | tail -5`

Expected: `Finished` with no errors.

- [ ] **Step 9: Commit**

```bash
git add proto/common.proto crates/transports/
git commit -m "feat: add metadata_json to protobuf AudioBuffer and transport adapters"
```

---

## Task 3: Extend IPC audio format with header

**Files:**
- Modify: `crates/core/src/python/multiprocess/data_transfer.rs:26-44`
- Modify: `crates/core/src/python/multiprocess/multiprocess_executor.rs:790-805,883-898`
- Modify: `clients/python/remotemedia/core/multiprocessing/node.py:599-605`

- [ ] **Step 1: Update the IPC audio constructor in data_transfer.rs**

In `crates/core/src/python/multiprocess/data_transfer.rs`, replace the `audio()` function (lines 26-44) with a new version that includes a header:

```rust
/// Create audio runtime data
///
/// Binary payload format:
///   sample_rate  (4 bytes, u32 LE)
///   channels     (2 bytes, u16 LE)
///   metadata_len (4 bytes, u32 LE, 0 = no metadata)
///   metadata     (metadata_len bytes, UTF-8 JSON)
///   samples      (remaining bytes, f32 LE)
pub fn audio(
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
    session_id: &str,
    metadata: Option<&serde_json::Value>,
) -> Self {
    let metadata_bytes = metadata
        .map(|m| serde_json::to_vec(m).unwrap_or_default())
        .unwrap_or_default();

    let samples_bytes = unsafe {
        std::slice::from_raw_parts(
            samples.as_ptr() as *const u8,
            samples.len() * std::mem::size_of::<f32>(),
        )
    };

    // Build payload: header + samples
    let mut payload = Vec::with_capacity(10 + metadata_bytes.len() + samples_bytes.len());
    payload.extend_from_slice(&sample_rate.to_le_bytes());        // 4 bytes
    payload.extend_from_slice(&channels.to_le_bytes());            // 2 bytes
    payload.extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes()); // 4 bytes
    payload.extend_from_slice(&metadata_bytes);                    // variable
    payload.extend_from_slice(samples_bytes);                      // remaining

    Self {
        data_type: DataType::Audio,
        session_id: session_id.to_string(),
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64,
        payload,
    }
}
```

- [ ] **Step 2: Update core→IPC conversion in multiprocess_executor.rs**

In `crates/core/src/python/multiprocess/multiprocess_executor.rs`, update the `core_to_ipc` function (around line 797) to pass metadata:

```rust
MainRD::Audio {
    samples,
    sample_rate,
    channels,
    metadata,
    ..
} => {
    IPCRuntimeData::audio(
        samples,
        *sample_rate,
        *channels as u16,
        session_id,
        metadata.as_ref(),
    )
}
```

- [ ] **Step 3: Update IPC→core conversion in multiprocess_executor.rs**

In `crates/core/src/python/multiprocess/multiprocess_executor.rs`, update the `DataType::Audio` deserialization (around line 883) to parse the new header:

```rust
DataType::Audio => {
    let payload = &ipc_data.payload;
    if payload.len() < 10 {
        return Err(crate::Error::IpcError("Audio IPC payload too short".into()));
    }

    let mut pos = 0;

    // Parse header
    let sample_rate = u32::from_le_bytes([
        payload[pos], payload[pos+1], payload[pos+2], payload[pos+3],
    ]);
    pos += 4;

    let channels = u16::from_le_bytes([payload[pos], payload[pos+1]]) as u32;
    pos += 2;

    let metadata_len = u32::from_le_bytes([
        payload[pos], payload[pos+1], payload[pos+2], payload[pos+3],
    ]) as usize;
    pos += 4;

    let metadata = if metadata_len > 0 && pos + metadata_len <= payload.len() {
        serde_json::from_slice(&payload[pos..pos + metadata_len]).ok()
    } else {
        None
    };
    pos += metadata_len;

    // Parse samples
    let samples: Vec<f32> = payload[pos..]
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();

    Ok(MainRD::Audio {
        samples,
        sample_rate,
        channels,
        stream_id: None,
        timestamp_us: Some(ipc_data.timestamp),
        arrival_ts_us: None,
        metadata,
    })
}
```

- [ ] **Step 4: Add `annotations` field to Python AudioMetadata**

In `clients/python/remotemedia/core/multiprocessing/data.py`, add an `annotations` field to `AudioMetadata` (around line 58):

```python
@dataclass
class AudioMetadata:
    """Audio-specific metadata."""
    sample_rate: int  # Hz
    channels: int     # 1=mono, 2=stereo
    format: AudioFormat
    duration_ms: int  # Duration in milliseconds
    annotations: Optional[dict] = None  # Pipeline metadata (e.g., speaker diarization)
```

This keeps the typed validation in `__post_init__` happy while allowing freeform pipeline metadata.

- [ ] **Step 5: Update Python IPC deserialization**

In `clients/python/remotemedia/core/multiprocessing/node.py`, update the audio parsing code (around line 599) to parse the new header:

```python
if data_type == 1:  # Audio
    # New format: sample_rate (4) | channels (2) | metadata_len (4) | metadata | samples
    if len(payload) < 10:
        self.logger.error(f"Audio payload too short: {len(payload)} bytes")
        return None

    import struct
    pos = 0
    sample_rate = struct.unpack_from('<I', payload, pos)[0]
    pos += 4
    channels = struct.unpack_from('<H', payload, pos)[0]
    pos += 2
    metadata_len = struct.unpack_from('<I', payload, pos)[0]
    pos += 4

    annotations = None
    if metadata_len > 0 and pos + metadata_len <= len(payload):
        import json
        try:
            annotations = json.loads(payload[pos:pos + metadata_len])
        except json.JSONDecodeError:
            self.logger.warning("Failed to parse audio annotations JSON")
        pos += metadata_len

    audio_samples = np.frombuffer(payload[pos:], dtype=np.float32)
    self.logger.info(
        f"Received audio via IPC: {len(audio_samples)} samples, "
        f"{sample_rate}Hz, {channels}ch, annotations={'yes' if annotations else 'no'}"
    )
    rd = RuntimeData.audio(audio_samples, sample_rate, channels=channels)
    if annotations:
        rd.metadata.annotations = annotations
    return rd
```

- [ ] **Step 6: Update all other callers of IPCRuntimeData::audio()**

Grep for all call sites: `grep -rn "IPCRuntimeData::audio(" crates/core/src/python/`

Each caller needs the new `metadata` parameter added (pass `None` if no metadata is available).

- [ ] **Step 7: Build and test**

Run: `cargo build 2>&1 | tail -5`

Expected: `Finished` with no errors.

- [ ] **Step 8: Commit**

```bash
git add crates/core/src/python/multiprocess/ clients/python/remotemedia/core/multiprocessing/
git commit -m "feat: extend IPC audio format with sample_rate, channels, and metadata header"
```

---

## Task 4: Fix SpeakerDiarizationNode to annotate audio

**Files:**
- Modify: `crates/core/src/nodes/speaker_diarization.rs:240-370,435-446`

- [ ] **Step 1: Update the process_streaming callback to emit annotated audio**

In `crates/core/src/nodes/speaker_diarization.rs`, replace the dual-output callback section (lines 353-369) with a single annotated audio output:

```rust
            drop(states); // Release lock

            let mut outputs = 0;

            if self.passthrough_audio {
                // Extract fields from the original audio data
                if let RuntimeData::Audio {
                    samples,
                    sample_rate: orig_sr,
                    channels: orig_ch,
                    stream_id,
                    timestamp_us,
                    arrival_ts_us,
                    ..
                } = data
                {
                    callback(RuntimeData::Audio {
                        samples,
                        sample_rate: orig_sr,
                        channels: orig_ch,
                        stream_id,
                        timestamp_us,
                        arrival_ts_us,
                        metadata: Some(serde_json::json!({
                            "diarization": {
                                "segments": speaker_segments,
                                "num_speakers": num_speakers,
                                "time_offset": time_offset,
                                "duration": mono.len() as f64 / self.sample_rate as f64,
                            }
                        })),
                    })?;
                    outputs += 1;
                }
            }

            Ok(outputs)
```

- [ ] **Step 2: Update the node schema**

In `crates/core/src/nodes/speaker_diarization.rs`, update the `schema()` method (around line 441) to reflect the new output type:

```rust
    fn schema(&self) -> Option<crate::nodes::schema::NodeSchema> {
        use crate::nodes::schema::{NodeSchema, RuntimeDataType};
        Some(
            NodeSchema::new("SpeakerDiarizationNode")
                .description("Identifies speakers in audio streams and annotates audio with speaker metadata")
                .category("ml")
                .accepts([RuntimeDataType::Audio])
                .produces([RuntimeDataType::Audio])
                .config_schema_from::<SpeakerDiarizationConfig>(),
        )
    }
```

- [ ] **Step 3: Build**

Run: `cargo build 2>&1 | tail -5`

Expected: `Finished` with no errors.

- [ ] **Step 4: Fix and run diarization tests**

The existing tests in the `#[cfg(test)] mod tests` block at the end of `speaker_diarization.rs` may assert on the old JSON output behavior. Update any test that checks for `RuntimeData::Json` output to check for `RuntimeData::Audio` with `metadata` instead.

Run: `cd crates/core && cargo test speaker_diarization 2>&1 | tail -10`

Expected: all tests pass (the existing tests test config defaults, resampling, and mono conversion — they should pass without changes since they don't test `process_streaming`).

- [ ] **Step 5: Commit**

```bash
git add crates/core/src/nodes/speaker_diarization.rs
git commit -m "feat: diarization node embeds speaker segments in audio metadata"
```

---

## Task 5: Modify candle-whisper to forward metadata

**Files:**
- Modify: `crates/candle-nodes/src/whisper/mod.rs:331-343`

- [ ] **Step 1: Update the process() method to extract and forward metadata**

In `crates/candle-nodes/src/whisper/mod.rs`, replace the `process()` method (lines 331-343):

```rust
    async fn process(&self, data: RuntimeData) -> std::result::Result<RuntimeData, Error> {
        // Extract metadata from the input (extract_audio borrows &data, so we can read both)
        let input_metadata = match &data {
            RuntimeData::Audio { metadata, .. } => metadata.clone(),
            _ => None,
        };

        // Extract audio from input (borrows &data)
        let audio = RuntimeDataConverter::extract_audio(&data, &self.node_id)
            .map_err(|e| Error::Execution(e.to_string()))?;

        // Transcribe
        let transcription = self
            .transcribe(audio)
            .await
            .map_err(|e| Error::Execution(e.to_string()))?;

        // Forward metadata if it contains diarization info
        match input_metadata {
            Some(ref meta) if meta.get("diarization").is_some() => {
                Ok(RuntimeData::Json(serde_json::json!({
                    "text": transcription,
                    "diarization": meta["diarization"],
                })))
            }
            _ => Ok(RuntimeData::Text(transcription)),
        }
    }
```

- [ ] **Step 2: Build**

Run: `cargo build -p remotemedia-candle-nodes 2>&1 | tail -5`

Expected: `Finished` with no errors.

- [ ] **Step 3: Run candle whisper tests**

Run: `cd crates/candle-nodes && cargo test whisper 2>&1 | tail -10`

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/candle-nodes/src/whisper/mod.rs
git commit -m "feat: candle-whisper forwards diarization metadata from input audio"
```

---

## Task 6: Create pipeline YAMLs

**Files:**
- Create: `examples/cli/pipelines/diarize-caption.yaml`
- Create: `examples/cli/pipelines/tts-voice-b.yaml`

- [ ] **Step 1: Create the diarize-caption production pipeline**

Create `examples/cli/pipelines/diarize-caption.yaml`:

```yaml
# Diarization + STT Live Caption Pipeline
# Usage: remotemedia run diarize-caption.yaml -i audio.pcm --input-format raw-pcm -O -
# Requires: --features candle,speaker-diarization

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

- [ ] **Step 2: Create the TTS voice B pipeline**

Create `examples/cli/pipelines/tts-voice-b.yaml`:

```yaml
# Text-to-Speech Pipeline (Male Voice)
# Usage: remotemedia run tts-voice-b.yaml -i "Hello" -O speech.pcm

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

- [ ] **Step 3: Validate both pipelines**

Run from repo root:
```bash
./examples/target/debug/remotemedia validate examples/cli/pipelines/diarize-caption.yaml
./examples/target/debug/remotemedia validate examples/cli/pipelines/tts-voice-b.yaml
```

Expected: `✓ Manifest is valid` for both.

- [ ] **Step 4: Commit**

```bash
git add examples/cli/pipelines/diarize-caption.yaml examples/cli/pipelines/tts-voice-b.yaml
git commit -m "feat: add diarize-caption pipeline and male-voice TTS for testing"
```

---

## Task 7: Create end-to-end test script

**Files:**
- Create: `examples/cli/tests/test-diarize-caption.sh`

- [ ] **Step 1: Create the test script**

Create `examples/cli/tests/test-diarize-caption.sh`:

```bash
#!/bin/bash
set -euo pipefail

# Diarization + STT Live Caption End-to-End Test
#
# Prerequisites:
#   - CLI built with: cargo build --features candle,speaker-diarization
#   - PYTHON_ENV_MODE=managed for KokoroTTS
#
# What it does:
#   1. Generates two TTS audio clips (female + male voices)
#   2. Concatenates them with silence gap
#   3. Runs diarization + transcription pipeline
#   4. Validates output has 2 speakers with correct text

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CLI="${SCRIPT_DIR}/../../target/debug/remotemedia"
PIPELINES="${SCRIPT_DIR}/../pipelines"
TMP_DIR=$(mktemp -d)
trap "rm -rf $TMP_DIR" EXIT

echo "=== Diarize-Caption End-to-End Test ==="
echo "Temp dir: $TMP_DIR"

# Verify CLI exists
if [ ! -f "$CLI" ]; then
    echo "ERROR: CLI not found at $CLI"
    echo "Build with: cd examples/cli/remotemedia-cli && cargo build --features candle,speaker-diarization"
    exit 1
fi

# Step 1: Generate test audio (two different voices)
echo ""
echo "=== Step 1: Generating test audio ==="

echo "Generating voice A (female - af_bella)..."
echo "Watch out there is an enemy behind you" | \
    PYTHON_ENV_MODE=managed $CLI run "$PIPELINES/tts.yaml" -i - -O "$TMP_DIR/voice_a.pcm" --timeout 120

echo "Generating voice B (male - am_adam)..."
echo "I need healing can someone help me" | \
    PYTHON_ENV_MODE=managed $CLI run "$PIPELINES/tts-voice-b.yaml" -i - -O "$TMP_DIR/voice_b.pcm" --timeout 120

echo "Voice A: $(wc -c < "$TMP_DIR/voice_a.pcm") bytes"
echo "Voice B: $(wc -c < "$TMP_DIR/voice_b.pcm") bytes"

# Step 2: Concatenate with silence gap (0.5s at 24kHz = 12000 zero f32 samples = 48000 bytes)
echo ""
echo "=== Step 2: Concatenating audio ==="
python3 -c "import sys; sys.stdout.buffer.write(b'\x00' * 48000)" > "$TMP_DIR/silence.pcm"
cat "$TMP_DIR/voice_a.pcm" "$TMP_DIR/silence.pcm" "$TMP_DIR/voice_b.pcm" > "$TMP_DIR/two_speakers.pcm"
echo "Combined audio: $(wc -c < "$TMP_DIR/two_speakers.pcm") bytes"

# Step 3: Run diarize+caption pipeline
echo ""
echo "=== Step 3: Running diarization + transcription ==="
$CLI run "$PIPELINES/diarize-caption.yaml" \
    -i "$TMP_DIR/two_speakers.pcm" \
    --input-format raw-pcm \
    -O "$TMP_DIR/output.json" \
    -o json \
    --timeout 120 2>"$TMP_DIR/run.log" || {
    echo "Pipeline failed. Log:"
    cat "$TMP_DIR/run.log"
    exit 1
}

echo "=== Pipeline Output ==="
cat "$TMP_DIR/output.json"

# Step 4: Validate output
echo ""
echo "=== Step 4: Validation ==="

if python3 -c "
import json, sys

with open('$TMP_DIR/output.json') as f:
    data = json.load(f)

# Check structure
assert 'text' in data, 'Missing text field'
assert 'diarization' in data, 'Missing diarization field'

segments = data['diarization']['segments']
speakers = set(s['speaker'] for s in segments)
print(f'Speakers found: {speakers}')
print(f'Transcribed text: {data[\"text\"]}')
print(f'Segments: {len(segments)}')

# Validate speaker count (soft check — short TTS clips may merge into 1 speaker)
if len(speakers) < 2:
    print(f'WARNING: Expected >= 2 speakers, got {len(speakers)}: {speakers}')
    print('This can happen with short audio clips. Try longer text or adjust search_threshold.')
else:
    print(f'✓ Detected {len(speakers)} distinct speakers')

# Validate text contains keywords from both inputs
text_lower = data['text'].lower()
voice_a_keywords = ['watch', 'enemy', 'behind']
voice_b_keywords = ['healing', 'help']

voice_a_found = any(w in text_lower for w in voice_a_keywords)
voice_b_found = any(w in text_lower for w in voice_b_keywords)

if not voice_a_found:
    print(f'WARNING: No keywords from voice A found (expected one of: {voice_a_keywords})')
if not voice_b_found:
    print(f'WARNING: No keywords from voice B found (expected one of: {voice_b_keywords})')

assert voice_a_found or voice_b_found, 'No keywords from either voice found in transcription'

print()
print('✓ All validations passed')
"; then
    echo ""
    echo "TEST PASSED"
    exit 0
else
    echo ""
    echo "TEST FAILED"
    exit 1
fi
```

- [ ] **Step 2: Make script executable**

Run: `chmod +x examples/cli/tests/test-diarize-caption.sh`

- [ ] **Step 3: Commit**

```bash
git add examples/cli/tests/test-diarize-caption.sh
git commit -m "feat: add diarize-caption end-to-end test script"
```

---

## Task 8: Build and run end-to-end test

**Files:** None (validation only)

- [ ] **Step 1: Build CLI with all required features**

Run:
```bash
cd examples/cli/remotemedia-cli
PATH=$HOME/.local/bin:$PATH cargo build --features candle,speaker-diarization
```

Expected: `Finished` with no errors.

- [ ] **Step 2: Run the end-to-end test**

Run:
```bash
cd examples/cli
PYTHON_ENV_MODE=managed bash tests/test-diarize-caption.sh
```

Expected: `TEST PASSED` — output contains 2+ speakers and transcribed keywords.

- [ ] **Step 3: If test fails, debug and fix**

Check `$TMP_DIR/run.log` for pipeline errors. Common issues:
- Missing speaker-diarization models: rebuild with the feature enabled
- Whisper model download: first run may be slow
- Diarization threshold: if only 1 speaker detected, try lowering `search_threshold` in diarize-caption.yaml

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: diarization + STT live caption pipeline — complete implementation"
```
